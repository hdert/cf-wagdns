use envfile::EnvFile;
use log::{debug, info, warn, LevelFilter};
use serde_json::Value;
use simplelog::*;
use std::fs::File;
// use std::fs::OpenOptions;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
#[derive(Debug, Serialize, Deserialize)]
struct CloudflareResponse {
    errors: Vec<CloudflareError>,
    messages: Option<Vec<Value>>,
    result: Option<Vec<HashMap<String, Value>>>,
    result_info: Option<HashMap<String, i32>>,
    success: bool,
}
#[derive(Debug, Serialize, Deserialize)]
struct CloudflareError {
    code: i32,
    message: String,
}

fn main() {
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

    let current_ip = get_ip().unwrap();
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
    // let (zone_identifier, record_identifier, env_file) =
    // get_zone_record_identifiers(&mut env_file, token, &config_file);
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
    // let (account_identifier, group_identifier) =
    // get_account_group_identifiers(token, env_file, &config_file);
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
                config_file.get("GROUP_NAME").unwrap(),
            )
            .unwrap()
            // get_account_group_ids_from_cloudflare(
            //     bypass_token,
            //     config_file
            //         .get("ACCOUNT_NAME")
            //         .expect("Error: ACCOUNT_NAME not set"),
            //     config_file
            //         .get("GROUP_NAME")
            //         .expect("Error: GROUP_NAME not set"),
            // )
        }
    };
    debug!("Account id: {account_identifier}");
    debug!("Group id: {group_identifier}");

    // println!("{zone_identifier} {record_identifier}");
    // println!("{account_identifier} {group_identifier}");
}

// fn get_zone_record_identifiers<'a>(
//     env_file: &'a mut EnvFile,
//     token: &str,
//     config_file: &EnvFile,
// ) -> (String, String, &'a mut EnvFile) {
//     debug!("Entered get_zone_record_identifiers");
//     let zone_id = env_file.get("ZONE_ID");
//     let record_id = env_file.get("RECORD_ID");
//     debug!("Zone id prelim: {zone_id:#?}");
//     debug!("Record id prelim: {record_id:#?}");

//     match (zone_id, record_id) {
//         (Some(result_1), Some(result_2)) => {
//             debug!("From file was valid");
//             (result_1.to_string(), result_2.to_string(), env_file)
//         }
//         (Some(result_1), None) => {
//             debug!("Left valid from file");
//             (
//                 result_1.to_string(),
//                 get_record_id_from_cloudflare(
//                     result_1,
//                     config_file
//                         .get("RECORD_NAME")
//                         .expect("Error: RECORD_NAME not set."),
//                     token,
//                 ),
//                 env_file,
//             )
//         }
//         (None, None) | (None, Some(_)) => {
//             debug!("None valid");
//             match get_zone_record_ids_from_cloudflare(
//                 token,
//                 config_file
//                     .get("ZONE_NAME")
//                     .expect("Error: ZONE_NAME not set"),
//                 config_file
//                     .get("RECORD_NAME")
//                     .expect("Error: RECORD_NAME not set."),
//             ) {
//                 (result_1, result_2) => (result_1, result_2, env_file),
//             }
//         }
//     }
// }

fn get_zone_record_ids_from_cloudflare(
    token: &str,
    zone_name: &str,
    record_name: &str,
) -> Result<(String, String), reqwest::Error> {
    debug!("Entered get_zone_record_ids_from_cloudflare");
    let response = cloudflare_get(
        token,
        format!("https://api.cloudflare.com/client/v4/zones?name={zone_name}"),
    )?;
    let result = response.result.unwrap();
    let zone_id = result[0]["id"].as_str().unwrap();
    debug!("get_zone_record_ids_from_cloudflare: Zone id is: {zone_id:#?}");
    Ok((
        zone_id.to_owned(),
        get_record_id_from_cloudflare(zone_id, record_name, token)?,
    ))
}

fn get_record_id_from_cloudflare(
    zone_id: &str,
    record_name: &str,
    token: &str,
) -> Result<String, reqwest::Error> {
    debug!("Entered get_record_id_from_cloudflare");
    let response = cloudflare_get(
        token,
        format!(
            "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records?name={record_name}"
        ),
    )?;
    Ok(response.result.unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_owned())
}

fn cloudflare_get(token: &str, url: String) -> Result<CloudflareResponse, reqwest::Error> {
    debug!("Entered cloudflare_get");
    debug!("cloudflare_get: Url is: {url:#?}");
    let response: CloudflareResponse = reqwest::blocking::Client::new()
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send()?
        .json()
        .unwrap();
    debug!("cloudflare_get: Response is: {:#?}", response);
    // TODO: Check response validity: Zero length errors, non-zero length response
    Ok(response)
}

// fn get_account_group_identifiers(
//     token: &str,
//     env_file: &mut EnvFile,
//     config_file: &EnvFile,
// ) -> (String, String) {
//     debug!("Entered get_account_group_identifiers");
//     let account_id = env_file.get("ACCOUNT_ID");
//     let group_id = env_file.get("GROUP_ID");
//     debug!("Account id prelim: {account_id:#?}");
//     debug!("Group id prelim: {group_id:#?}");

//     match (account_id, group_id) {
//         (Some(result_1), Some(result_2)) => {
//             debug!("From file was valid");
//             (result_1.to_string(), result_2.to_string())
//         }
//         (Some(result), None) => {
//             debug!("Left valid from file");
//             (
//                 result.to_string(),
//                 get_group_id_from_cloudflare(token, result, config_file.get("GROUP_NAME").unwrap()),
//             )
//         }
//         (None, None) | (None, Some(_)) => {
//             debug!("None valid");
//             get_account_group_ids_from_cloudflare(
//                 token,
//                 config_file
//                     .get("ACCOUNT_NAME")
//                     .expect("Error: ACCOUNT_NAME not set"),
//                 config_file
//                     .get("GROUP_NAME")
//                     .expect("Error: GROUP_NAME not set"),
//             )
//         }
//     }
// }

// fn get_account_group_ids_from_cloudflare(
//     token: &str,
//     account_name: &str,
//     group_name: &str,
// ) -> (String, String) {
//     debug!("Entered get_account_group_ids_from_cloudflare");
//     let response = cloudflare_get(
//         token,
//         "https://api.cloudflare.com/client/v4/accounts".to_string(),
//     );
//     // let account_id = response["result"][0]["id"].as_str().unwrap();
//     let account_id = "placeholder1";
//     debug!("Account group ids response: {response:#?}");
//     debug!("Account id is: {account_id:#?}");
//     (
//         account_id.to_string(),
//         get_group_id_from_cloudflare(token, account_id, group_name),
//     )
// }

fn get_group_id_from_cloudflare(
    token: &str,
    account_id: &str,
    group_name: &str,
) -> Result<String, reqwest::Error> {
    debug!("Entered get_group_id_from_cloudflare");
    let response = cloudflare_get(
        token,
        format!("https://api.cloudflare.com/client/v4/accounts/{account_id}/access/groups"),
    )?;
    let result = response.result.unwrap();
    debug!("{:#?}", result);
    Ok(result
        .iter()
        .filter(|value| value["name"].as_str().unwrap() == group_name)
        .collect::<Vec<&HashMap<String, Value>>>()[0]["id"]
        .as_str()
        .unwrap()
        .to_owned())
    // let v = response["result"].as_array().unwrap().clone();
    // let mut result: String = format!("Uninitialized");
    // for i in v {
    //     debug!("{i:#?}");
    //     if i["name"].as_str().unwrap() == group_name {
    //         return Some(i["id"].as_str().unwrap().to_string());
    //     }
    // }
    // return None;
}

fn get_ip() -> Result<String, reqwest::Error> {
    let mut resp = reqwest::blocking::get("https://ipv4.icanhazip.com")?.text()?;
    resp.pop();
    debug!("IP response: {resp:#?}");

    Ok(resp)
}
