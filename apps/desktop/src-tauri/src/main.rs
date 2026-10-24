#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lev_core::{
    ApiKeyLocation, Auth, Body, Collection, Method, RequestDef, Scripts, VarSet, VarValue,
};
use lev_engine::{convert, in_process_client, Engine};
use lev_format::{load_collection, load_request, save_collection, save_request, Yaml};
use lev_http::Timings;
use lev_protocol::EnvDto;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct SendRequestCmd {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body_type: String,
    body_content: String,
    auth_type: Option<String>,
    auth_data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct SendResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    timings: Timings,
}

#[tauri::command]
async fn send_request(cmd: SendRequestCmd) -> Result<SendResponse, String> {
    let engine = Engine::with_os_secrets().map_err(|e| e.to_string())?;
    let client = in_process_client(engine).await.map_err(|e| e.to_string())?;

    let method = Method::parse(&cmd.method);

    let body = match cmd.body_type.as_str() {
        "json" => {
            if cmd.body_content.is_empty() {
                Body::None
            } else {
                Body::Json(cmd.body_content.clone())
            }
        }
        "raw" => {
            if cmd.body_content.is_empty() {
                Body::None
            } else {
                Body::Raw {
                    content_type: None,
                    content: cmd.body_content.clone(),
                }
            }
        }
        "form" => Body::Form(vec![]),
        _ => Body::None,
    };

    let mut headers = cmd.headers.clone();
    let auth = match cmd.auth_type.as_deref() {
        Some("bearer") => {
            if let Some(data) = &cmd.auth_data {
                if let Some(token) = data.get("token").and_then(|v| v.as_str()) {
                    Some(Auth::Bearer(lev_core::Template::parse(token).map_err(|e| e.to_string())?))
                } else {
                    None
                }
            } else {
                None
            }
        }
        Some("basic") => {
            if let Some(data) = &cmd.auth_data {
                let username = data.get("username").and_then(|v| v.as_str()).unwrap_or("");
                let password = data.get("password").and_then(|v| v.as_str()).unwrap_or("");
                Some(Auth::Basic {
                    username: lev_core::Template::parse(username).map_err(|e| e.to_string())?,
                    password: VarValue::Plain(password.to_string()),
                })
            } else {
                None
            }
        }
        Some("apikey") => {
            if let Some(data) = &cmd.auth_data {
                let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let value = data.get("value").and_then(|v| v.as_str()).unwrap_or("");
                let location = match data.get("in").and_then(|v| v.as_str()) {
                    Some("query") => ApiKeyLocation::Query,
                    _ => ApiKeyLocation::Header,
                };
                Some(Auth::ApiKey {
                    name: name.to_string(),
                    value: VarValue::Plain(value.to_string()),
                    location,
                })
            } else {
                None
            }
        }
        _ => None,
    };

    let lev_req = RequestDef {
        id: "temp".to_string(),
        name: "temp".to_string(),
        method,
        url: lev_core::Template::parse(&cmd.url).map_err(|e| e.to_string())?,
        headers: headers
            .into_iter()
            .map(|(k, v)| lev_core::model::Header { name: k, value: v })
            .collect(),
        body,
        auth,
        scripts: Scripts::default(),
        assertions: vec![],
        timeout: None,
    };

    let collection = Collection {
        id: "temp".to_string(),
        name: "temp".to_string(),
        base_url: None,
        auth: None,
        headers: vec![],
    };

    let vars = VarSet::new();
    let env = EnvDto {
        collection: convert::collection_to_dto(&collection),
        vars: convert::vars_to_dto(&vars),
        seed: 0,
    };
    let req_dto = convert::request_to_dto(&lev_req);

    let outcome = client.send(req_dto, env).await.map_err(|e| e.to_string())?;

    Ok(SendResponse {
        status: outcome.response.status,
        headers: outcome.response.headers,
        body: outcome.response.body,
        timings: outcome.response.timings,
    })
}

#[tauri::command]
async fn load_collection_cmd(path: String) -> Result<Collection, String> {
    let collection = load_collection(&Yaml, std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(collection)
}

#[tauri::command]
async fn load_request_cmd(path: String) -> Result<RequestDef, String> {
    let req = load_request(&Yaml, std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(req)
}

#[tauri::command]
async fn save_collection_cmd(path: String, collection: Collection) -> Result<(), String> {
    save_collection(&Yaml, std::path::Path::new(&path), &collection).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn save_request_cmd(path: String, request: RequestDef) -> Result<(), String> {
    save_request(&Yaml, std::path::Path::new(&path), &request).map_err(|e| e.to_string())?;
    Ok(())
}

#[tokio::main]
async fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            send_request,
            load_collection_cmd,
            load_request_cmd,
            save_collection_cmd,
            save_request_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
