use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::fs;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{transport::Server, Request, Response, Status};
use uuid::Uuid;
use axum::{extract::Path as AxumPath, extract::State as AxumState, response::IntoResponse, routing::get, Router};

use proto::screencapture::screen_capture_service_server::{ScreenCaptureService, ScreenCaptureServiceServer};
use proto::screencapture::screen_capture_service_client::ScreenCaptureServiceClient;
use proto::screencapture::{AgentRegistration, CaptureCommand, ScreenshotResponse, SubmitAck};

use xcap::Monitor;
use device_query::{DeviceQuery, DeviceState};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all available monitors
    List,
    /// Start the unified Server (HTTP + gRPC hub)
    Serve {
        #[arg(long, default_value_t = 50051)]
        grpc_port: u16,
        #[arg(long, default_value_t = 8080)]
        http_port: u16,
    },
    /// Connect this machine as a remote agent
    Agent {
        #[arg(long)]
        id: String,
        #[arg(long)]
        server: String, // e.g: http://127.0.0.1:50051
        /// Optional: Immediately capture this monitor and push to server, instead of listening
        #[arg(long)]
        capture: Option<usize>,
    },
    /// Local legacy capture commands
    Local {
        #[arg(long, global = true)]
        wait_for_keys: bool,
        #[command(subcommand)]
        cmd: LocalCommands,
    }
}

#[derive(Subcommand)]
enum LocalCommands {
    /// List all available monitors
    List,
    /// Capture a region from a monitor (waits for shortcut from .keys)
    Capture {
        #[arg(long)]
        monitor: usize,
        #[arg(long)]
        x: i32,
        #[arg(long)]
        y: i32,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long)]
        output: PathBuf,
    },
    /// Capture the entire monitor (waits for shortcut from .keys)
    CaptureFull {
        #[arg(long)]
        monitor: usize,
        #[arg(long)]
        output: PathBuf,
    },
    /// Continuously take screenshots when mouse is inside the region (waits for shortcut from .keys)
    WhenMouseIn {
        #[arg(long)]
        monitor: usize,
        #[arg(long)]
        x: i32,
        #[arg(long)]
        y: i32,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long)]
        output_prefix: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run_cli(cli).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run_cli(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Commands::List => {
            match Monitor::all() {
                Ok(monitors) => {
                    for (i, m) in monitors.iter().enumerate() {
                        let name = m.name().unwrap_or_else(|_| "<unknown>".to_string());
                        let w = m.width().unwrap_or(0);
                        let h = m.height().unwrap_or(0);
                        println!("{}: {} ({}x{})", i, name, w, h);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to enumerate monitors: {e}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Serve { grpc_port, http_port } => {
            run_server(grpc_port, http_port).await?;
        }
        Commands::Agent { id, server, capture } => {
            run_agent(id, server, capture).await?;
        }
        Commands::Local { wait_for_keys, cmd } => {
            run_local(wait_for_keys, cmd).await?;
        }
    }
    Ok(())
}

// =======================
// SERVER MODE
// =======================

type AgentTx = mpsc::Sender<Result<CaptureCommand, Status>>;
type PendingCaptures = Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<Vec<u8>>>>>;

#[derive(Clone)]
struct AppState {
    agents: Arc<Mutex<std::collections::HashMap<String, AgentTx>>>,
    pending: PendingCaptures,
}

struct MyScreenCaptureService {
    app_state: AppState,
}

#[tonic::async_trait]
impl ScreenCaptureService for MyScreenCaptureService {
    type ConnectAgentStream = ReceiverStream<Result<CaptureCommand, Status>>;

    async fn connect_agent(
        &self,
        request: Request<AgentRegistration>,
    ) -> Result<Response<Self::ConnectAgentStream>, Status> {
        let agent_id = request.into_inner().agent_id;
        println!("Agent connected: {}", agent_id);

        let (tx, rx) = mpsc::channel(4);
        self.app_state.agents.lock().await.insert(agent_id, tx);

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn submit_screenshot(
        &self,
        request: Request<ScreenshotResponse>,
    ) -> Result<Response<SubmitAck>, Status> {
        let req = request.into_inner();
        let cmd_id = req.command_id.clone();
        
        let mut pending = self.app_state.pending.lock().await;
        let mut handled = false;
        if let Some(tx) = pending.remove(&cmd_id) {
            let _ = tx.send(req.image_data.clone());
            handled = true;
        }

        if !handled {
            let dir = std::path::Path::new("received_images");
            if !dir.exists() {
                let _ = std::fs::create_dir_all(dir);
            }
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
            let safe_filename = if cmd_id.is_empty() {
                format!("screenshot_push_{}.png", now)
            } else {
                format!("screenshot_push_{}_{}.png", cmd_id, now)
            };
            let file_path = dir.join(&safe_filename);
            if let Ok(_) = std::fs::write(&file_path, &req.image_data) {
                println!("Got push screenshot, saved to {:?}", file_path);
            }
        }

        Ok(Response::new(SubmitAck { received: true }))
    }
}

async fn capture_handler(
    AxumPath(agent_id): AxumPath<String>,
    AxumState(state): AxumState<AppState>,
) -> impl IntoResponse {
    let cmd_id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();

    state.pending.lock().await.insert(cmd_id.clone(), tx);

    let sent = {
        let agents = state.agents.lock().await;
        if let Some(agent_tx) = agents.get(&agent_id) {
            agent_tx.send(Ok(CaptureCommand {
                command_id: cmd_id.clone(),
                monitor_idx: 0,
            })).await.is_ok()
        } else {
            false
        }
    };

    if !sent {
        state.pending.lock().await.remove(&cmd_id);
        return (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            "Agent not found or disconnected".into_response()
        ).into_response();
    }

    match tokio::time::timeout(Duration::from_secs(15), rx).await {
        Ok(Ok(image_data)) => {
            (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "image/png")],
                image_data
            ).into_response()
        }
        _ => {
            state.pending.lock().await.remove(&cmd_id);
            (
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                [(axum::http::header::CONTENT_TYPE, "text/plain")],
                "Timeout waiting for agent to capture screen".into_response()
            ).into_response()
        }
    }
}

async fn run_server(grpc_port: u16, http_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        agents: Arc::new(Mutex::new(std::collections::HashMap::new())),
        pending: Arc::new(Mutex::new(std::collections::HashMap::new())),
    };

    let grpc_addr = format!("0.0.0.0:{}", grpc_port).parse()?;
    let grpc_service = MyScreenCaptureService { app_state: state.clone() };

    println!("Starting Server Modes...");
    println!("gRPC listening on: {}", grpc_addr);
    let grpc_future = Server::builder()
        .add_service(ScreenCaptureServiceServer::new(grpc_service))
        .serve(grpc_addr);

    let http_addr: std::net::SocketAddr = format!("0.0.0.0:{}", http_port).parse()?;
    let app = Router::new()
        .route("/api/v1/capture/:agent_id", get(capture_handler))
        .with_state(state);
        
    println!("HTTP listening on: http://{}", http_addr);
    let http_future = axum::Server::bind(&http_addr).serve(app.into_make_service());

    tokio::select! {
        _ = grpc_future => {},
        _ = http_future => {},
    }

    Ok(())
}

// =======================
// AGENT MODE
// =======================

async fn run_agent(id: String, server_url: String, capture: Option<usize>) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = ScreenCaptureServiceClient::connect(server_url.clone()).await?;

    if let Some(mon_idx) = capture {
        let monitors = Monitor::all().unwrap_or_default();
        if mon_idx < monitors.len() {
            let m = &monitors[mon_idx];
            let width = m.width().unwrap_or(0);
            let height = m.height().unwrap_or(0);
            match m.capture_region(0, 0, width, height) {
                Ok(img) => {
                    let mut bytes: Vec<u8> = Vec::new();
                    let mut cursor = std::io::Cursor::new(&mut bytes);
                    if let Err(e) = img.write_to(&mut cursor, image::ImageFormat::Png) {
                        eprintln!("Failed to encode to Png: {}", e);
                    } else {
                        let res = ScreenshotResponse {
                            command_id: String::new(),
                            image_data: bytes,
                            success: true,
                            error_message: String::new(),
                        };
                        println!("Pushing capture for monitor {} via gRPC...", mon_idx);
                        if let Err(e) = client.submit_screenshot(tonic::Request::new(res)).await {
                            eprintln!("Failed to push capture: {}", e);
                        } else {
                            println!("Push successful!");
                        }
                    }
                }
                Err(e) => eprintln!("Failed to capture: {}", e),
            }
        } else {
            eprintln!("Monitor index {} out of bounds", mon_idx);
        }
        return Ok(());
    }

    let req = tonic::Request::new(AgentRegistration { agent_id: id.clone() });
    
    let mut stream = client.connect_agent(req).await?.into_inner();
    println!("Connected to gRPC server at {}. Waiting for capture commands...", server_url);

    while let Some(msg) = stream.next().await {
        let msg = msg?;
        let cmd_id = msg.command_id;
        let mon_idx = msg.monitor_idx as usize;
        
        println!("Received capture command {}. Capturing monitor {}", cmd_id, mon_idx);
        
        let monitors = Monitor::all().unwrap_or_default();
        if mon_idx < monitors.len() {
            let m = &monitors[mon_idx];
            let width = m.width().unwrap_or(0);
            let height = m.height().unwrap_or(0);
            
            match m.capture_region(0, 0, width, height) {
                Ok(img) => {
                    let mut bytes: Vec<u8> = Vec::new();
                    let mut cursor = std::io::Cursor::new(&mut bytes);
                    if let Err(e) = img.write_to(&mut cursor, image::ImageFormat::Png) {
                        eprintln!("Failed to encode to Png: {}", e);
                    } else {
                        let res = ScreenshotResponse {
                            command_id: cmd_id.clone(),
                            image_data: bytes,
                            success: true,
                            error_message: String::new(),
                        };
                        let mut temp_client = ScreenCaptureServiceClient::connect(server_url.clone()).await?;
                        if let Err(e) = temp_client.submit_screenshot(tonic::Request::new(res)).await {
                            eprintln!("Failed to upload capture: {}", e);
                        } else {
                            println!("Capture {} uploaded successfully.", cmd_id);
                        }
                    }
                }
                Err(e) => eprintln!("Failed to capture: {}", e),
            }
        } else {
            eprintln!("Command requested monitor {}, but only {} are available.", mon_idx, monitors.len());
        }
    }
    
    Ok(())
}

// =======================
// LOCAL LEGACY CAPTURE MODE
// =======================

async fn run_local(wait_for_keys: bool, cmd: LocalCommands) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        LocalCommands::List => {
            match Monitor::all() {
                Ok(monitors) => {
                    for (i, m) in monitors.iter().enumerate() {
                        let name = m.name().unwrap_or_else(|_| "<unknown>".to_string());
                        let w = m.width().unwrap_or(0);
                        let h = m.height().unwrap_or(0);
                        println!("{}: {} ({}x{})", i, name, w, h);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to enumerate monitors: {e}");
                    std::process::exit(1);
                }
            }
        }
        LocalCommands::Capture { monitor, x, y, width, height, output } => {
            let m = get_monitor(monitor);
            if cli_wait(wait_for_keys).await {
                capture_and_save(&m, x as u32, y as u32, width, height, &output).await;
            }
        }
        LocalCommands::CaptureFull { monitor, output } => {
            let m = get_monitor(monitor);
            let w = m.width().unwrap_or(0);
            let h = m.height().unwrap_or(0);
            if cli_wait(wait_for_keys).await {
                capture_and_save(&m, 0, 0, w, h, &output).await;
            }
        }
        LocalCommands::WhenMouseIn { monitor, x, y, width, height, output_prefix } => {
            let m = get_monitor(monitor);
            let device_state = DeviceState::new();
            if wait_for_keys {
                let shortcut_keys = read_keys();
                println!("Monitoring mouse position. Waiting for shortcut {:?}. Press Ctrl+C to exit.", shortcut_keys);
                let mut prev_image: Option<Vec<u8>> = None;
                loop {
                    let mouse = device_state.get_mouse();
                    let (mx, my) = (mouse.coords.0, mouse.coords.1);
                    if mx >= x && my >= y && (mx as u32) < (x as u32 + width) && (my as u32) < (y as u32 + height) {
                        let pressed = device_state.get_keys();
                        if shortcut_keys.iter().all(|k| pressed.contains(k)) {
                            match m.capture_region(x as u32, y as u32, width, height) {
                                Ok(img) => {
                                    let raw = img.as_raw();
                                    let is_new = match &prev_image {
                                        Some(prev) => prev != raw,
                                        None => true,
                                    };
                                    if is_new {
                                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
                                        let filename = output_prefix.with_extension("").to_string_lossy().to_string() + &format!("_{}.png", now);
                                        let file_path = PathBuf::from(filename);
                                        if let Err(e) = img.save(&file_path) {
                                            eprintln!("Failed to save image: {}", e);
                                        } else {
                                            println!("Screenshot saved to {} (mouse at {}, {})", file_path.display(), mx, my);
                                            prev_image = Some(raw.clone());
                                        }
                                    }
                                }
                                Err(e) => eprintln!("Failed to capture region: {}", e),
                            }
                            while shortcut_keys.iter().all(|k| device_state.get_keys().contains(k)) {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            } else {
                let mouse = device_state.get_mouse();
                let (mx, my) = (mouse.coords.0, mouse.coords.1);
                if mx >= x && my >= y && (mx as u32) < (x as u32 + width) && (my as u32) < (y as u32 + height) {
                    match m.capture_region(x as u32, y as u32, width, height) {
                        Ok(img) => {
                            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
                            let filename = output_prefix.with_extension("").to_string_lossy().to_string() + &format!("_{}.png", now);
                            let file_path = PathBuf::from(filename);
                            if let Err(e) = img.save(&file_path) {
                                eprintln!("Failed to save image: {}", e);
                            } else {
                                println!("Screenshot saved to {} (mouse at {}, {})", file_path.display(), mx, my);
                            }
                        }
                        Err(e) => eprintln!("Failed to capture region: {}", e),
                    }
                } else {
                    println!("Mouse is not in the specified region. No screenshot taken.");
                }
            }
        }
    }
    Ok(())
}

fn get_monitor(idx: usize) -> Monitor {
    let monitors = Monitor::all().unwrap_or_default();
    if idx >= monitors.len() {
        eprintln!("Monitor index {} out of bounds", idx);
        std::process::exit(1);
    }
    monitors[idx].clone()
}

fn read_keys() -> Vec<device_query::Keycode> {
    use device_query::Keycode;
    let shortcut_str = match fs::read_to_string(".keys") {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            eprintln!("Failed to read .keys file: {}", e);
            std::process::exit(1);
        }
    };
    let shortcut_keys: Vec<Keycode> = shortcut_str.split('+').filter_map(|k| {
        match k.trim().to_uppercase().as_str() {
            "CTRL" => Some(Keycode::LControl), "SHIFT" => Some(Keycode::LShift), "ALT" => Some(Keycode::LAlt),
            "CMD" | "COMMAND" | "META" => Some(Keycode::Meta),
            "A" => Some(Keycode::A), "B" => Some(Keycode::B), "C" => Some(Keycode::C), "D" => Some(Keycode::D),
            "E" => Some(Keycode::E), "F" => Some(Keycode::F), "G" => Some(Keycode::G), "H" => Some(Keycode::H),
            "I" => Some(Keycode::I), "J" => Some(Keycode::J), "K" => Some(Keycode::K), "L" => Some(Keycode::L),
            "M" => Some(Keycode::M), "N" => Some(Keycode::N), "O" => Some(Keycode::O), "P" => Some(Keycode::P),
            "Q" => Some(Keycode::Q), "R" => Some(Keycode::R), "S" => Some(Keycode::S), "T" => Some(Keycode::T),
            "U" => Some(Keycode::U), "V" => Some(Keycode::V), "W" => Some(Keycode::W), "X" => Some(Keycode::X),
            "Y" => Some(Keycode::Y), "Z" => Some(Keycode::Z),
            "0" => Some(Keycode::Key0), "1" => Some(Keycode::Key1), "2" => Some(Keycode::Key2), "3" => Some(Keycode::Key3),
            "4" => Some(Keycode::Key4), "5" => Some(Keycode::Key5), "6" => Some(Keycode::Key6), "7" => Some(Keycode::Key7),
            "8" => Some(Keycode::Key8), "9" => Some(Keycode::Key9),
            _ => None
        }
    }).collect();
    if shortcut_keys.is_empty() {
        eprintln!("No valid keys in .keys file");
        std::process::exit(1);
    }
    shortcut_keys
}

async fn cli_wait(wait_for_keys: bool) -> bool {
    if !wait_for_keys {
        return true;
    }
    let shortcut_keys = read_keys();
    let device_state = DeviceState::new();
    println!("Waiting for shortcut {:?}. Press Ctrl+C to exit.", shortcut_keys);
    loop {
        let pressed = device_state.get_keys();
        if shortcut_keys.iter().all(|k| pressed.contains(k)) {
            while shortcut_keys.iter().all(|k| device_state.get_keys().contains(k)) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            return true;
        } else {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

async fn capture_and_save(m: &Monitor, x: u32, y: u32, width: u32, height: u32, output: &PathBuf) {
    match m.capture_region(x, y, width, height) {
        Ok(img) => {
            if let Err(e) = img.save(output) {
                eprintln!("Failed to save image: {}", e);
            } else {
                println!("Screenshot saved to {}", output.display());
            }
        }
        Err(e) => {
            eprintln!("Failed to capture region: {}", e);
        }
    }
}
