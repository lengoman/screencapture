use proto::screencapture::screen_capture_service_server::{ScreenCaptureService, ScreenCaptureServiceServer};
use proto::screencapture::{ScreenshotRequest, ScreenshotResponse};
use tonic::{transport::Server, Request, Response, Status};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub struct MyScreenCaptureService {}

#[tonic::async_trait]
impl ScreenCaptureService for MyScreenCaptureService {
    async fn send_screenshot(
        &self,
        request: Request<ScreenshotRequest>,
    ) -> Result<Response<ScreenshotResponse>, Status> {
        let req = request.into_inner();
        
        let dir = Path::new("received_images");
        if !dir.exists() {
            fs::create_dir_all(dir).unwrap_or_else(|e| eprintln!("Failed to create dir: {}", e));
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        let safe_filename = if req.filename.is_empty() {
            format!("screenshot_{}.png", now)
        } else {
            // Very basic sanitation: just replace slashes to avoid directory traversal
            req.filename.replace("/", "_").replace("\\", "_")
        };
        
        let file_path = dir.join(&safe_filename);
        
        match fs::write(&file_path, &req.image_data) {
            Ok(_) => {
                println!("Got screenshot: {}, saved to {:?}", safe_filename, file_path);
                Ok(Response::new(ScreenshotResponse {
                    success: true,
                    message: format!("Successfully saved as {}", safe_filename),
                }))
            }
            Err(e) => {
                eprintln!("Failed to save screenshot: {}", e);
                Err(Status::internal(format!("Failed to save: {}", e)))
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:50051".parse()?;
    let service = MyScreenCaptureService::default();

    println!("ScreenCapture gRPC Server listening on {}", addr);

    Server::builder()
        .add_service(ScreenCaptureServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
