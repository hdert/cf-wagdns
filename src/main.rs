//! A simple Cloudflare dns record updater script, with helpful error messages and simple configuration,
//! and the ability to update Access Group rules.

use envfile::EnvFile;
use error_stack::{Report, Result, ResultExt};
use log::{debug, info, warn, LevelFilter};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};
use std::fs::{File, OpenOptions};
use std::{collections::HashMap, error::Error, fmt, path::Path};
// use std::fs::File;

#[tokio::main]
async fn main() {
    let mut env_file = EnvFile::new(Path::new(".env")).expect("Error: No .env file");
    let config_file =
        EnvFile::new(Path::new("cf-wagdns.config")).expect("Error: no cf-wagdns.config file");
    let log_file_path = config_file
        .get("LOG_FILE")
        .expect("Error: LOG_FILE not defined in cf-wagdns.config");
    let log_file = match OpenOptions::new().append(true).open(log_file_path) {
        Ok(file) => file,
        Err(_) => {
            File::create(log_file_path)
                .expect(&format!("Couldn't create a log file at {log_file_path}"));
            OpenOptions::new().append(true).open(log_file_path).unwrap()
        }
    };
    let log_config = ConfigBuilder::new().set_time_format_rfc2822().build();
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Warn,
            log_config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Debug,
            log_config,
            // File::create(
            //     config_file
            //         .get("LOG_FILE")
            //         .expect("Error: LOG_FILE not defined in cf-wagdns.config"),
            // )
            // .expect("Error: Was not able to create log file."),
            log_file,
        ), // TODO: Change to append(true) once program is in production
    ])
    .expect("Error: was not able to create logger instance");
    debug!("Program has started");
    // warn!("Still logging instead of append logging!");

    let current_ip = get_ip().await.unwrap();
    debug!("{current_ip}");

    if let Some(result) = env_file.get("IP_ADDRESS") {
        if result == current_ip {
            info!("IP unchanged");
            return;
            // warn!("Return statement removed, normal return would be here!");
        } else {
            env_file.update("IP_ADDRESS", &current_ip);
            env_file.write().unwrap();
            info!("Updated env file ip to {current_ip}");
        }
    } else {
        warn!("Historical IP not set, this should not be a problem if this your first time running this program.");
        env_file.update("IP_ADDRESS", &current_ip);
        env_file.write().unwrap();
        info!("Set env file ip to {current_ip}");
    }
    debug!("Guard clause over");

    let token = env_file.get("TOKEN").unwrap();
    let bypass_token = env_file.get("BYPASS_TOKEN").unwrap();
    let record_name = config_file
        .get("RECORD_NAME")
        .expect("Error: RECORD_NAME not set.");

    debug!("Grabbed tokens");
    let (zone_identifier, record_identifier) =
        match (env_file.get("ZONE_ID"), env_file.get("RECORD_ID")) {
            (Some(result_1), Some(result_2)) => {
                debug!("From file was valid");
                (result_1.to_owned(), result_2.to_owned())
            }
            (Some(result_1), None) => {
                debug!("Left valid from file");
                (
                    result_1.to_owned(),
                    get_record_id_from_cloudflare(result_1, record_name, token)
                        .await
                        .unwrap(),
                )
            }
            (None, None | Some(_)) => {
                debug!("None valid");
                get_zone_record_ids_from_cloudflare(
                    token,
                    config_file
                        .get("ZONE_NAME")
                        .expect("Error: ZONE_NAME not set"),
                    record_name,
                )
                .await
                .unwrap()
            }
        };
    debug!("Zone id: {zone_identifier}");
    debug!("Record id: {record_identifier}");

    let result = cloudflare_put(token, format!("https://api.cloudflare.com/client/v4/zones/{zone_identifier}/dns_records/{record_identifier}"), json!({"id": zone_identifier, "type": "A", "name": record_name, "content": current_ip})).await.unwrap();
    info!("Update IP to: {current_ip}");
    debug!("Result: {result:#?}");

    if config_file
        .get("UPDATE_ACCESS")
        .expect("Error: UPDATE_ACCESS not defined")
        != "true"
    {
        return;
    }

    let account_identifier = env_file.get("ACCOUNT_ID").expect("Error: No account ID");
    let group_identifier = if let Some(result) = env_file.get("GROUP_ID") {
        debug!("From file was valid");
        result.to_owned()
    } else {
        debug!("None valid");
        get_group_id_from_cloudflare(
            bypass_token,
            account_identifier,
            config_file
                .get("GROUP_NAME")
                .expect("Error: GROUP_NAME not set"),
        )
        .await
        .unwrap()
    };
    debug!("Account id: {account_identifier}");
    debug!("Group id: {group_identifier}");

    let result = cloudflare_get(
        bypass_token,
        format!(
            "https://api.cloudflare.com/client/v4/accounts/{account_identifier}/access/groups/{group_identifier}"
        ),
    )
    .await
    .unwrap();
    debug!("Result: {result:#?}");

    let result = replace_ip_in_result(&result, &current_ip).unwrap();
    debug!("Result: {result:#?}");

    let result = cloudflare_put(
        bypass_token,
        format!(
            "https://api.cloudflare.com/client/v4/accounts/{account_identifier}/access/groups/{group_identifier}"
        ),
        json!(result),
    )
    .await
    .unwrap();
    info!("Update Cloudflare Access IP to: {current_ip}");
    debug!("Result: {result:#?}");

    env_file.update("ZONE_ID", &zone_identifier);
    env_file.update("RECORD_ID", &record_identifier);
    env_file.update("GROUP_ID", &group_identifier);
    match env_file.write() {
        Ok(_) => (),
        Err(err) => warn!("Failed to update zone_id and record_id in .env:\n{err}"),
    }
}

/// Simple type to describe Access Group rules.
type Rules = Vec<HashMap<String, HashMap<String, String>>>;

fn replace_ip_in_result(
    result: &[HashMap<String, Value>],
    ip: &str,
) -> Result<HashMap<String, Value>, CloudflareError> {
    let mut response: HashMap<String, Rules> = HashMap::new();

    for value in ["include", "require", "exclude"] {
        if result[0].contains_key(value) {
            let mut rules: Rules = serde_json::from_value(
                result[0]
                    .get(value)
                    .ok_or_else(|| Report::new(CloudflareError::ParseError))?
                    .clone(),
            )
            .change_context(CloudflareError::ParseError)?;

            if ["include", "require"].contains(&value) {
                debug!("Does contain {value}");
                for rule in &mut rules {
                    if rule.contains_key("ip") {
                        rule.insert(
                            "ip".to_owned(),
                            HashMap::from([("ip".to_owned(), ip.to_string())]),
                        );
                        debug!("Did a replacement");
                    }
                }
            }

            response.insert(value.to_owned(), rules);
        }
    }

    debug!("{:#?}", response);

    let mut response: HashMap<String, Value> = serde_json::from_value(
        serde_json::to_value(response).change_context(CloudflareError::Unsuccessful)?,
    )
    .change_context(CloudflareError::Unsuccessful)?;

    response.insert(
        "name".to_owned(),
        result[0]
            .get("name")
            .ok_or_else(|| Report::new(CloudflareError::Unsuccessful))?
            .clone(),
    );

    debug!("{:#?}", response);

    debug!(
        "{:#?}",
        serde_json::to_string(&response).change_context(CloudflareError::Unsuccessful)?
    );

    Ok(response)
}

/// Internal enum to describe the type of error that has occured.
#[derive(Debug)]
enum CloudflareError {
    ReqwestError,
    Unsuccessful,
    EmptyResponse,
    ParseError,
}

/// Make printing the error type prettier
impl fmt::Display for CloudflareError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReqwestError => {
                fmt.write_str("Error getting data from Cloudflare: Reqwest failure")
            }
            Self::Unsuccessful => fmt.write_str("Error getting data from Cloudflare: Unsuccessful"),
            Self::EmptyResponse => {
                fmt.write_str("Error getting data from Cloudflare: Empty response")
            }
            Self::ParseError => {
                fmt.write_str("Error getting data from Cloudflare: Error parsing response")
            }
        }
    }
}

impl Error for CloudflareError {}

/// Internal struct to describe a cloudflare response
#[derive(Debug, Serialize, Deserialize)]
struct CloudflareResponse {
    errors: Vec<CloudflareResponseError>,
    messages: Option<Vec<Value>>,
    result: Option<CloudflareResult>,
    result_info: Option<HashMap<String, i32>>,
    success: bool,
}

/// Enum to describe result types, as cloudflare can return either a JSON array
/// or a JSON object in the result, which maps to rust's vectors and hashmaps respectively.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum CloudflareResult {
    Vec(Vec<HashMap<String, Value>>),
    HashMap(HashMap<String, Value>),
}

/// Nice self contained type to describe an error.
#[derive(Debug, Serialize, Deserialize)]
struct CloudflareResponseError {
    code: i32,
    message: String,
}

/// Make printing the error from cloudflare prettier.
impl fmt::Display for CloudflareResponseError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(&format!(
            "Cloudflare API call failed with code {} and message:\n{}",
            self.code, self.message
        ))
    }
}

impl Error for CloudflareResponseError {}

/// Make a put request to the url in url with the token and data given and parse the response.
async fn cloudflare_put(
    token: &str,
    url: String,
    data: Value,
) -> Result<Vec<HashMap<String, Value>>, CloudflareError> {
    debug!("Entered cloudflare_put");
    debug!("cloudflare_put: Url is {url:#?} data is:\n{data:#?}");
    let body = serde_json::to_string(&data).unwrap();
    debug!("{body:#?}");
    let response = reqwest::Client::new()
        .put(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .change_context(CloudflareError::ReqwestError)?
        .json()
        .await
        .change_context(CloudflareError::ParseError)?;

    debug!("cloudflare_put: Response is: {response:#?}");

    parse_result(response)
}

/// Make a get request to the url given with the token given and parse the response.
async fn cloudflare_get(
    token: &str,
    url: String,
) -> Result<Vec<HashMap<String, Value>>, CloudflareError> {
    debug!("Entered cloudflare_get");
    debug!("cloudflare_get: Url is: {url:#?}");
    let response = reqwest::Client::new()
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .change_context(CloudflareError::ReqwestError)
        .attach_printable("About to parse for JSON")?
        .json()
        .await
        .change_context(CloudflareError::ParseError)
        .attach_printable("Failed to parse JSON")?;

    // let response: CloudflareResponse = response
    //     .json()
    //     .await
    //     .into_report()
    //     .change_context(CloudflareError::ParseError)?;
    debug!("cloudflare_get: Response is: {:#?}", response);
    // TODO: Check response validity: Zero length errors, non-zero length response
    parse_result(response)
}

/// Parse the result from `cloudflare_put` and get, do some basic sanity checks like for the length of the result.
fn parse_result(
    response: CloudflareResponse,
) -> Result<Vec<HashMap<String, Value>>, CloudflareError> {
    debug!("Parse result: Not finished.");
    match response.result {
        Some(CloudflareResult::Vec(result)) => Ok(result),
        Some(CloudflareResult::HashMap(result)) => Ok(vec![result]),
        None => Err(Report::new(CloudflareError::EmptyResponse)),
    }
}

/// Get the ip from icanhazip.
async fn get_ip() -> Result<String, reqwest::Error> {
    let mut resp = reqwest::get("https://ipv4.icanhazip.com")
        .await?
        .text()
        .await?;
    resp.pop();
    debug!("IP response: {resp:#?}");

    Ok(resp)
}

/// Get the record id from cloudflare given the record name.
async fn get_record_id_from_cloudflare(
    zone_id: &str,
    record_name: &str,
    token: &str,
) -> Result<String, CloudflareError> {
    // debug!("Entered get_record_id_from_cloudflare");
    let result = cloudflare_get(
        token,
        format!(
            "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records?name={record_name}"
        ),
    )
    .await?;
    Ok(result[0]["id"]
        .as_str()
        .ok_or_else(|| Report::new(CloudflareError::ParseError))?
        .to_owned())
}

/// Get the zone and record ids from cloudflare given the zone and record names.
async fn get_zone_record_ids_from_cloudflare(
    token: &str,
    zone_name: &str,
    record_name: &str,
) -> Result<(String, String), CloudflareError> {
    // debug!("Entered get_zone_record_ids_from_cloudflare");
    let result = cloudflare_get(
        token,
        format!("https://api.cloudflare.com/client/v4/zones?name={zone_name}"),
    )
    .await?;
    let zone_id = result[0]["id"].as_str().unwrap();
    // debug!("get_zone_record_ids_from_cloudflare: Zone id is: {zone_id:#?}");
    Ok((
        zone_id.to_owned(),
        get_record_id_from_cloudflare(zone_id, record_name, token).await?,
    ))
}

/// Get the group id from cloudflare.
async fn get_group_id_from_cloudflare(
    token: &str,
    account_id: &str,
    group_name: &str,
) -> Result<String, CloudflareError> {
    // debug!("Entered get_group_id_from_cloudflare");
    let result = cloudflare_get(
        token,
        format!("https://api.cloudflare.com/client/v4/accounts/{account_id}/access/groups"),
    )
    .await?;
    // debug!("{:#?}", result);
    /* Although we are guaranteed that result is non-zero length by cloudflare_get(), we are not guaranteed
    that it is non-zero length after we've ran .filter() on it. */
    let filtered = result
        .iter()
        .filter(|value| value["name"].as_str().unwrap() == group_name)
        .collect::<Vec<&HashMap<String, Value>>>();

    let id = match filtered.len() {
        1 => match filtered[0]["id"].as_str() {
            Some(string) => string.to_owned(),
            None => return Err(Report::new(CloudflareError::ParseError)),
        },
        _ => {
            return Err(
                Report::new(CloudflareError::Unsuccessful).attach_printable(format!(
                    "Group name: {group_name} wasn't in in the list of results:\n{result:#?}"
                )),
            )
        }
    };

    Ok(id)
}
