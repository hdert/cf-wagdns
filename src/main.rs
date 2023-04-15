use envfile::EnvFile;
use error_stack::{IntoReport, Report, Result, ResultExt};
use log::{debug, info, warn, LevelFilter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use simplelog::*;
use std::{collections::HashMap, error::Error, fmt, fs::File, path::Path};
// use std::fs::OpenOptions;

#[tokio::main]
async fn main() {
    let mut env_file = EnvFile::new(Path::new(".env")).expect("Error: No .env file");
    let config_file = EnvFile::new(Path::new("cloudflare-ddns.config"))
        .expect("Error: no cloudflare-ddns.config file");
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Warn,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Debug,
            Config::default(),
            File::create(
                config_file
                    .get("LOG_FILE")
                    .expect("Error: LOG_FILE not defined in cloudflare-ddns.config"),
            )
            .expect("Error: Was not able to create log file"),
            // OpenOptions::new()
            // .append(true)
            // .open(config_file.get("LOG_FILE").unwrap())
            // .unwrap(),
        ), // TODO: Change to append(true) once program is in production
    ])
    .expect("Error: was not able to create logger instance");
    debug!("Program has started");

    let current_ip = get_ip().await.unwrap();
    debug!("{current_ip}");

    match env_file.get("IP_ADDRESS") {
        Some(result) => {
            if *result == current_ip {
                info!("IP unchanged");
                // return;
                warn!("Return statement removed, normal return would be here!")
            } else {
                env_file.update("IP_ADDRESS", &current_ip);
                env_file.write().unwrap();
                info!("Updated env file ip to {current_ip}");
            }
        }
        None => {
            warn!("Historical IP not set");
            env_file.update("IP_ADDRESS", &current_ip);
            env_file.write().unwrap();
            info!("Set env file ip to {current_ip}")
        }
    };
    debug!("Guard clause over");

    let token = env_file.get("TOKEN").unwrap();
    let bypass_token = env_file.get("BYPASS_TOKEN").unwrap();

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
                    get_record_id_from_cloudflare(
                        result_1,
                        config_file
                            .get("RECORD_NAME")
                            .expect("Error: RECORD_NAME not set."),
                        token,
                    )
                    .await
                    .unwrap(),
                )
            }
            (None, None) | (None, Some(_)) => {
                debug!("None valid");
                get_zone_record_ids_from_cloudflare(
                    token,
                    config_file
                        .get("ZONE_NAME")
                        .expect("Error: ZONE_NAME not set"),
                    config_file
                        .get("RECORD_NAME")
                        .expect("Error: RECORD_NAME not set."),
                )
                .await
                .unwrap()
            }
        };
    debug!("Zone id: {zone_identifier}");
    debug!("Record id: {record_identifier}");
    if config_file
        .get("UPDATE_ACCESS")
        .expect("Error: UPDATE_ACCESS not defined")
        != "true"
    {
        return;
    }

    let account_identifier = env_file.get("ACCOUNT_ID").expect("Error: No account ID");
    let group_identifier = match env_file.get("GROUP_ID") {
        Some(result) => {
            debug!("From file was valid");
            result.to_owned()
        }
        None => {
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
        }
    };
    debug!("Account id: {account_identifier}");
    debug!("Group id: {group_identifier}");
}

#[derive(Debug)]
enum CloudflareError {
    ReqwestError,
    Unsuccessful,
    EmptyResponse,
    ParseError,
}

impl fmt::Display for CloudflareError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Error getting data from Cloudflare")
    }
}

impl Error for CloudflareError {}

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareResponse {
    errors: Vec<CloudflareResponseError>,
    messages: Option<Vec<Value>>,
    result: Option<Vec<HashMap<String, Value>>>,
    result_info: Option<HashMap<String, i32>>,
    success: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudflareResponseError {
    code: i32,
    message: String,
}

impl fmt::Display for CloudflareResponseError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str(&format!(
            "Cloudflare API call failed with code {} and message:\n{}",
            self.code, self.message
        ))
    }
}

impl Error for CloudflareResponseError {}

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
        .into_report()
        .change_context(CloudflareError::ReqwestError)?
        .json()
        .await
        .into_report()
        .change_context(CloudflareError::ParseError)?;

    // let response: CloudflareResponse = response
    //     .json()
    //     .await
    //     .into_report()
    //     .change_context(CloudflareError::ParseError)?;
    debug!("cloudflare_get: Response is: {:#?}", response);
    // TODO: Check response validity: Zero length errors, non-zero length response
    Ok(parse_result(response)?)
}

fn parse_result(
    response: CloudflareResponse,
) -> Result<Vec<HashMap<String, Value>>, CloudflareError> {
    match response.result {
        Some(result) => Ok(result),
        None => Err(Report::new(CloudflareError::EmptyResponse)),
    }
}

async fn get_ip() -> Result<String, reqwest::Error> {
    let mut resp = reqwest::get("https://ipv4.icanhazip.com")
        .await?
        .text()
        .await?;
    resp.pop();
    debug!("IP response: {resp:#?}");

    Ok(resp)
}

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
        _ => return Err(Report::new(CloudflareError::ParseError)),
    };

    Ok(id)
}
