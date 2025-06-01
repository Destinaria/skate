use std::{collections::HashMap, process::exit, sync::Arc};
use axum::{extract::{ws::{Message, WebSocket}, Path, State, WebSocketUpgrade}, http::StatusCode, response, routing::{any, get, post}, Json, Router};
use tokio::{fs::read_to_string, net::TcpListener, select, sync::{mpsc, Mutex, RwLock}};
use serde::{Deserialize, Serialize};
use inquire::{Select, Text};

const ROOT_SCRIPT: &str = include_str!("./script.js");
const ROOT_STYLE:  &str = include_str!("./style.css");

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
struct Dimensions {
    width: u16,
    height: u16
}

#[derive(Debug, Clone)]
struct Args {
    config: Option<Arc<str>>,
    port: Option<u16>,
    password: Option<Arc<str>>,
    control: bool
}

impl Args {
    pub fn parse() -> Self {
        let mut args = std::env::args().skip(2);
        let mut config = None;
        let mut port = None;
        let mut password = None;
        let mut control = false;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    if config.is_some() {
                        eprintln!("Config file already specified.");
                        exit(1)
                    }
                    if let Some(path) = args.next() {
                        config = Some(Arc::from(path.as_str()));
                    } else {
                        eprintln!("Config file path not specified.");
                        exit(1)
                    }
                }
                "--port" => {
                    if port.is_some() {
                        eprintln!("Port already specified.");
                        exit(1)
                    }
                    if let Some(port_str) = args.next() {
                        port = Some(port_str.parse().unwrap());
                    } else {
                        eprintln!("Port not specified.");
                        exit(1)
                    }
                }
                "--password" => {
                    if password.is_some() {
                        eprintln!("Password already specified.");
                        exit(1)
                    }
                    if let Some(pass) = args.next() {
                        password = Some(Arc::from(pass.as_str()));
                    } else {
                        eprintln!("Password not specified.");
                        exit(1)
                    }
                }
                "--control" => {
                    if control {
                        eprintln!("Control already enabled.");
                        exit(1)
                    }
                    control = true
                }
                _ => {
                    eprintln!("Unknown argument: {arg}");

                }
            }
        }
        Self {
            config,
            port,
            password,
            control
        }
    }
}

fn get_bg() -> Option<String> {
    Some("#111122".to_string())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Config {
    name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    control: Option<bool>,
    slides: Vec<String>,
    slide_ratio: Dimensions,
    #[serde(default = "get_bg")]
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<String>
}

#[derive(Debug, Clone)]
struct ServerState {
    name: Arc<str>,
    slides: Arc<[Arc<str>]>,
    slide_ratio: Dimensions,
    password: Arc<str>,
    control: bool,
    background: Arc<str>,
    counter: Arc<Mutex<u16>>,
    websockets: Arc<RwLock<HashMap<u16, mpsc::Sender<String>>>>
}

fn display_help(program: &str) {
    println!("Usage: {program} <action> [options]
Actions:                                            
    help  - Print this message and quit
    init  - Create a new skate presentation interactively
    on    - Start the presentation server
        --config <PATH>        - The path to the config file (default: ./skate.json)
        --port <PORT>          - The port to serve the presentation on
        --password <PASS>      - The password for switching slides (overrides config file password)
        --control              - Allow control of the presentation without password (overrides config file control)");
}

async fn root(State(state): State<Arc<ServerState>>) -> response::Html<String> {
    response::Html(format!(r#"
<html>
    <head>
        <title>{name}</title>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="description" content={name}/>
        <style>
{style}
        </style>
    </head>
    <body>
        <iframe src="/0">
        <script>
{ROOT_SCRIPT}{}
        </script>
    </body>
</html>
"#,
        if state.control { "()" } else { "" },
        name  = state.name.clone(),
        style = ROOT_STYLE
                        .replace("BACKGROUND", &state.background.clone())
                        .replace("ASPECT_RATIO", &format!("{}/{}", state.slide_ratio.width, state.slide_ratio.height))
    ))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<ServerState>) {
    let mut counter = state.counter.lock().await;
    let mut websockets = state.websockets.write().await;
    let (sender, mut receiver) = mpsc::channel(1);
    let id = *counter;
    *counter += 1;
    websockets.insert(id, sender);
    drop(websockets);
    drop(counter);
    loop {
        select!{
            Some(message) = receiver.recv() => {
                socket.send(Message::Text(message.into())).await.unwrap();
            }
            Some(Ok(message)) = socket.recv() => {
                match message {
                    Message::Close(_) => {
                        state.websockets.write().await.remove(&id);
                        return
                    }
                    Message::Ping(msg) => {
                        let _ = socket.send(Message::Pong(msg)).await;
                    }
                    _ => ()
                }
            }
            else => break
        }
    }
    state.websockets.write().await.remove(&id);
}

async fn connect(ws: WebSocketUpgrade, State(state): State<Arc<ServerState>>) -> impl axum::response::IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn page(State(state): State<Arc<ServerState>>, Path(page): Path<usize>) -> (StatusCode, response::Html<String>) {
    let page = page.checked_rem(state.slides.len()).unwrap_or(0);
    let content = read_to_string(&*state.slides[page]).await;
    match content {
        Ok(content) => (StatusCode::OK, response::Html(content)),
        Err(e) => (StatusCode::NOT_FOUND, response::Html(format!("<html><body><h1>{e}</h1></body></html>")))
    }
}

#[derive(Deserialize)]
struct GotoMessage {
    page: usize,
    password: String
}

async fn goto(body: GotoMessage, state: Arc<ServerState>) -> StatusCode {
    if state.password == body.password.into() {
        let websockets = state.websockets.read().await;
        for (_, socket) in websockets.iter() {
            let _ = socket.send(body.page.to_string()).await;
        }
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args();
    let program = args.next().unwrap();

    match args.next().as_deref() {
        Some("init") => {
            let path = std::env::current_dir();
            let name_default = match path {
                Ok(ref path) => path.file_name().unwrap_or_default().to_str().unwrap_or_default(),
                Err(_) => "skate"
            };
            let name = Text::new("Project name:")
                .with_default(name_default)
                .prompt().unwrap();
            if name.is_empty() {
                eprintln!("Project name cannot be empty.");
                exit(1)
            }
            let password = Text::new("Password:").with_placeholder("Leave empty for no remote slide control.").prompt().unwrap();
            let control = Select::new("Allow client slide control:", vec!["Yes", "No"]).prompt().unwrap() == "Yes";
            let background = Text::new("Background:").with_default("#111122").prompt();
            let mut slides = Vec::new();
            while let Some(slide) = Some(Text::new("Slide:").with_placeholder("Leave empty to finish list.").prompt().unwrap()).filter(|s| !s.is_empty()) {
                slides.push(slide)
            }
            let slide_ratio = match Select::new("Slide ratio:", vec!["16:9", "4:3", "1:1"]).prompt().unwrap() {
                "16:9" => Dimensions { width: 16, height: 9 },
                "4:3" => Dimensions { width: 4, height: 3 },
                "1:1" => Dimensions { width: 1, height: 1 },
                _ => unreachable!()
            };
            let config = Config {
                name,
                password: if password.is_empty() { None } else { Some(password) },
                control: if control { Some(control) } else { None },
                slides,
                slide_ratio,
                background: background.ok()
            };
            let config_path = std::env::current_dir().unwrap().join("skate.json");
            let config_str = serde_json::to_string_pretty(&config).unwrap();
            std::fs::write(&config_path, config_str).unwrap();
            println!("Created config file at {}. Make your slides and {program} on!", config_path.to_string_lossy());
        }
        Some("on") => {
            let args = Args::parse();
            let config: Config = if let Some(path) = &args.config {
                let config_str = std::fs::read_to_string(path.to_string()).unwrap_or_else(|_| panic!("Failed to read config file: {path}"));
                serde_json::from_str(&config_str).unwrap()
            } else {
                let config_path = std::env::current_dir().unwrap().join("skate.json");
                let config_str = std::fs::read_to_string(&config_path).unwrap_or_else(|_| panic!("Failed to read config file: {}", config_path.to_string_lossy()));
                serde_json::from_str(&config_str).unwrap()
            };
            let config = Arc::new(config);
            let port = args.port.unwrap_or(3000);
            let password = args.password.clone().or(config.password.clone().map(|h| h.as_str().into())).unwrap_or_default();
            let control = args.control || config.control.unwrap_or(false);
            let slides = config.slides.iter().map(|s| Arc::from(s.as_str())).collect();
            let state = Arc::new(ServerState {
                name: config.name.clone().into(),
                slides,
                slide_ratio: config.slide_ratio,
                password,
                control,
                counter: Arc::new(Mutex::new(0)),
                background: config.background.clone().unwrap_or("#111122".into()).into(),
                websockets: Default::default()
            });
            let app = Router::new()
                .route("/", get(root))
                .route("/connect", any(connect))
                .route("/goto", post({
                    let state = Arc::clone(&state);
                    move |Json(body): Json<GotoMessage>| goto(body, state)
                }))
                .route("/page/{page}", get(page))
                .fallback_service(tower_http::services::ServeDir::new("."))
                .with_state(state);

            let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
            println!("Listening on http://localhost:{port}");
            axum::serve(listener, app).await.unwrap();
        }
        Some("--help" | "-h" | "help") | None => display_help(&program),
        Some(action) => {
            eprintln!("Unknown action: {action}.\nrun '{program} help' for help.");
        }
    }
}
