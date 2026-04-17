use clap::{Parser, Subcommand};
use xcap::Monitor;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use device_query::{DeviceQuery, DeviceState};
use std::fs;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Wait for commands interactively after startup (uses .keys)
    #[arg(long, global = true)]
    wait_for_keys: bool,
    
    /// Optional gRPC server URL to stream screenshots to (e.g., http://127.0.0.1:50051)
    #[arg(long, global = true)]
    grpc_url: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all available monitors
    List,
    /// Capture a region from a monitor (waits for shortcut from .keys)
    Capture {
        /// Monitor index (from list)
        #[arg(long)]
        monitor: usize,
        /// X coordinate (top-left)
        #[arg(long)]
        x: i32,
        /// Y coordinate (top-left)
        #[arg(long)]
        y: i32,
        /// Width of region
        #[arg(long)]
        width: u32,
        /// Height of region
        #[arg(long)]
        height: u32,
        /// Output file path (PNG)
        #[arg(long)]
        output: PathBuf,
    },
    /// Capture the entire monitor (waits for shortcut from .keys)
    CaptureFull {
        /// Monitor index (from list)
        #[arg(long)]
        monitor: usize,
        /// Output file path (PNG)
        #[arg(long)]
        output: PathBuf,
    },
    /// Continuously take screenshots when mouse is inside the region (waits for shortcut from .keys)
    WhenMouseIn {
        /// Monitor index (from list)
        #[arg(long)]
        monitor: usize,
        /// X coordinate (top-left)
        #[arg(long)]
        x: i32,
        /// Y coordinate (top-left)
        #[arg(long)]
        y: i32,
        /// Width of region
        #[arg(long)]
        width: u32,
        /// Height of region
        #[arg(long)]
        height: u32,
        /// Output file prefix (PNG files will be named <prefix>_<timestamp>.png)
        #[arg(long)]
        output_prefix: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run_command(cli).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run_command(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let grpc_url = cli.grpc_url.clone();
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
        Commands::Capture { monitor, x, y, width, height, output } => {
            use device_query::Keycode;
            let monitors = match Monitor::all() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to enumerate monitors: {e}");
                    std::process::exit(1);
                }
            };
            if monitor >= monitors.len() {
                eprintln!("Monitor index {monitor} out of range (found {} monitors)", monitors.len());
                std::process::exit(1);
            }
            let m = &monitors[monitor];
            if x < 0 || y < 0 {
                eprintln!("x and y must be non-negative (got x={}, y={})", x, y);
                std::process::exit(1);
            }
            if cli.wait_for_keys {
                let shortcut_str = match fs::read_to_string(".keys") {
                    Ok(s) => s.trim().to_string(),
                    Err(e) => {
                        eprintln!("Failed to read .keys file: {e}");
                        std::process::exit(1);
                    }
                };
                let shortcut_keys: Vec<Keycode> = get_shortcut_keys(&shortcut_str);
                
                let device_state = DeviceState::new();
                println!("Waiting for shortcut {:?}. Press Ctrl+C to exit.", shortcut_keys);
                loop {
                    let pressed = device_state.get_keys();
                    if shortcut_keys.iter().all(|k| pressed.contains(k)) {
                        capture_and_save(m, x as u32, y as u32, width, height, &output, grpc_url.as_deref()).await;
                        // Wait for keys to be released
                        while shortcut_keys.iter().all(|k| device_state.get_keys().contains(k)) {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        break;
                    } else {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            } else {
                capture_and_save(m, x as u32, y as u32, width, height, &output, grpc_url.as_deref()).await;
            }
        }
        Commands::CaptureFull { monitor, output } => {
            use device_query::Keycode;
            let monitors = match Monitor::all() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to enumerate monitors: {e}");
                    std::process::exit(1);
                }
            };
            if monitor >= monitors.len() {
                eprintln!("Monitor index {monitor} out of range (found {} monitors)", monitors.len());
                std::process::exit(1);
            }
            let m = &monitors[monitor];
            let width = match m.width() {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("Failed to get monitor width: {e}");
                    std::process::exit(1);
                }
            };
            let height = match m.height() {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Failed to get monitor height: {e}");
                    std::process::exit(1);
                }
            };
            if cli.wait_for_keys {
                let shortcut_str = match fs::read_to_string(".keys") {
                    Ok(s) => s.trim().to_string(),
                    Err(e) => {
                        eprintln!("Failed to read .keys file: {e}");
                        std::process::exit(1);
                    }
                };
                let shortcut_keys: Vec<Keycode> = get_shortcut_keys(&shortcut_str);
                
                let device_state = DeviceState::new();
                println!("Waiting for shortcut {:?}. Press Ctrl+C to exit.", shortcut_keys);
                loop {
                    let pressed = device_state.get_keys();
                    if shortcut_keys.iter().all(|k| pressed.contains(k)) {
                        capture_and_save(m, 0, 0, width, height, &output, grpc_url.as_deref()).await;
                        while shortcut_keys.iter().all(|k| device_state.get_keys().contains(k)) {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        break;
                    } else {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            } else {
                capture_and_save(m, 0, 0, width, height, &output, grpc_url.as_deref()).await;
            }
        }
        Commands::WhenMouseIn { monitor, x, y, width, height, output_prefix } => {
            use device_query::Keycode;
            let monitors = match Monitor::all() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to enumerate monitors: {e}");
                    std::process::exit(1);
                }
            };
            if monitor >= monitors.len() {
                eprintln!("Monitor index {monitor} out of range (found {} monitors)", monitors.len());
                std::process::exit(1);
            }
            if x < 0 || y < 0 {
                eprintln!("x and y must be non-negative (got x={}, y={})", x, y);
                std::process::exit(1);
            }
            let m = &monitors[monitor];
            let device_state = DeviceState::new();
            if cli.wait_for_keys {
                let shortcut_str = match fs::read_to_string(".keys") {
                    Ok(s) => s.trim().to_string(),
                    Err(e) => {
                        eprintln!("Failed to read .keys file: {e}");
                        std::process::exit(1);
                    }
                };
                let shortcut_keys: Vec<Keycode> = get_shortcut_keys(&shortcut_str);
                
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
                                            eprintln!("Failed to save image: {e}");
                                        } else {
                                            println!("Screenshot saved to {} (mouse at {}, {})", file_path.display(), mx, my);
                                            prev_image = Some(raw.clone());
                                            if let Some(url) = grpc_url.as_deref() {
                                                let fn_name = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                                send_via_grpc(url, &file_path, &fn_name).await;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to capture region: {e}");
                                }
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
                                eprintln!("Failed to save image: {e}");
                            } else {
                                println!("Screenshot saved to {} (mouse at {}, {})", file_path.display(), mx, my);
                                if let Some(url) = grpc_url.as_deref() {
                                    let fn_name = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                    send_via_grpc(url, &file_path, &fn_name).await;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to capture region: {e}");
                        }
                    }
                } else {
                    println!("Mouse is not in the specified region. No screenshot taken.");
                }
            }
        }
    }
    Ok(())
}

async fn capture_and_save(m: &Monitor, x: u32, y: u32, width: u32, height: u32, output: &PathBuf, grpc_url: Option<&str>) {
    match m.capture_region(x, y, width, height) {
        Ok(img) => {
            if let Err(e) = img.save(output) {
                eprintln!("Failed to save image: {e}");
            } else {
                println!("Screenshot saved to {}", output.display());
                if let Some(url) = grpc_url {
                    let fn_name = output.file_name().unwrap_or_default().to_string_lossy().to_string();
                    send_via_grpc(url, output, &fn_name).await;
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to capture region: {e}");
        }
    }
}

async fn send_via_grpc(grpc_url: &str, file_path: &Path, filename: &str) {
    use proto::screencapture::screen_capture_service_client::ScreenCaptureServiceClient;
    use proto::screencapture::ScreenshotRequest;

    match std::fs::read(file_path) {
        Ok(image_data) => {
            let mut client = match ScreenCaptureServiceClient::connect(grpc_url.to_string()).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to connect to gRPC server: {}", e);
                    return;
                }
            };
            
            let request = tonic::Request::new(ScreenshotRequest {
                image_data,
                filename: filename.to_string(),
                timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64,
            });

            match client.send_screenshot(request).await {
                Ok(response) => {
                    println!("gRPC server response: {:?}", response.into_inner().message);
                }
                Err(e) => {
                    eprintln!("Failed to send screenshot via gRPC: {}", e);
                }
            }
        }
        Err(e) => eprintln!("Could not read saved image for gRPC transmission: {}", e),
    }
}

fn get_shortcut_keys(shortcut_str: &str) -> Vec<device_query::Keycode> {
    use device_query::Keycode;
    let shortcut_keys: Vec<Keycode> = shortcut_str.split('+').filter_map(|k| {
        match k.trim().to_uppercase().as_str() {
            "CTRL" => Some(Keycode::LControl),
            "SHIFT" => Some(Keycode::LShift),
            "ALT" => Some(Keycode::LAlt),
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
        eprintln!("No valid keys found in .keys file");
        std::process::exit(1);
    }
    shortcut_keys
}
