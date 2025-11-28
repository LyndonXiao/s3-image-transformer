mod cache;
mod s3_client;
mod image_processor;

use anyhow::Result;
use bytes::Bytes;
use config::Config as ConfigLoader;
use serde::Deserialize;
use std::collections::HashMap;
use warp::{http::{Response, StatusCode}, Filter};

use crate::{
    cache::{ImageCache, CacheConfig},
    s3_client::{S3Client, S3Config},
    image_processor::{ImageProcessor, ImageProcessingConfig, parse_query_params},
};

#[derive(Debug, Deserialize, Clone)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize, Clone)]
struct AppConfig {
    server: ServerConfig,
    s3: S3Config,
    cache: CacheConfig,
    image_processing: ImageProcessingConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 解析命令行参数以支持 -c/--config <file>
    let mut args = std::env::args_os();
    // 跳过程序名
    args.next();
    let mut config_path: Option<std::path::PathBuf> = None;
    while let Some(arg) = args.next() {
        if arg == "-c" || arg == "--config" {
            if let Some(p) = args.next() {
                config_path = Some(std::path::PathBuf::from(p));
            }
        } else if let Some(s) = arg.to_str() {
            if s.starts_with("-c=") {
                if let Some(val) = s.splitn(2, '=').nth(1) {
                    config_path = Some(std::path::PathBuf::from(val));
                }
            }
        }
    }

    // 如果未指定，默认使用可执行文件同目录下的 `config.yaml`（回退到当前工作目录）
    let config_file = if let Some(p) = config_path {
        p
    } else {
        let exe = std::env::current_exe().unwrap_or_else(|_| std::env::current_dir().unwrap());
        let exe_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
        exe_dir.join("config.yaml")
    };

    // 加载配置文件（支持指定完整路径或默认的 config.yaml）
    let config_loader = ConfigLoader::builder()
        .add_source(config::File::from(config_file.as_path()))
        .build()?;

    let app_config: AppConfig = config_loader.try_deserialize()?;

    println!("Starting S3 Image Processor Server with Moka Cache...");
    println!("Listening on {}:{}", app_config.server.host, app_config.server.port);
    println!("Cache configuration: {}MB max, {}s TTL", 
        app_config.cache.max_capacity_mb, app_config.cache.time_to_live_sec);

    // 初始化缓存
    let cache = ImageCache::new(app_config.cache.clone());
    
    // 初始化S3客户端
    let s3_client = S3Client::new(app_config.s3.clone()).await?;
    
    // 初始化图片处理器
    let image_processor = ImageProcessor::new(
        s3_client, 
        cache,
        app_config.image_processing.clone()
    );

    // 创建路由
    let image_route = warp::path::tail()
        .and(warp::get().or(warp::head()).unify())
        .and(warp::query::<HashMap<String, String>>())
        .and_then({
            let processor = image_processor.clone();
            move |image_key: warp::filters::path::Tail, params: HashMap<String, String>| {
                let processor = processor.clone();
                let image_key = image_key.as_str().to_string();
                async move {
                    let processing_params = parse_query_params(params);
                    match processor.get_or_process_image(image_key, processing_params).await {
                        Ok((data, content_type, source)) => {
                            let response = Response::builder()
                                .header("Content-Type", content_type)
                                .header("X-Image-Source", source)
                                .header("Cache-Control", "public, max-age=3600")
                                .body(Bytes::from(data))
                                .unwrap();
                            Ok::<Response<bytes::Bytes>, warp::Rejection>(response)
                        }
                        Err(e) => {
                            eprintln!("Image processing error: {}", e);
                            Ok(Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(Bytes::from("Image not found"))
                                .unwrap())
                        }
                    }
                }
            }
        });

    let health_route = warp::path!("health").map(|| "OK");
    
    let stats_route = warp::path!("stats").map({
        let processor = image_processor.clone();
        move || {
            let stats = processor.get_cache_stats();
            format!("{}\n", stats)
        }
    });
    
    let clear_cache_route = warp::path!("clear-cache")
        .and(warp::post())
        .and_then({
            let processor = image_processor.clone();
            move || {
                let p = processor.clone();
                async move {
                    // 调用 ImageProcessor 提供的清理方法
                    p.clear_cache().await;
                    Ok::<String, warp::Rejection>("Cache cleared\n".to_string())
                }
            }
        });

    let routes = image_route
        .or(health_route)
        .or(stats_route)
        .or(clear_cache_route)
        .with(warp::cors().allow_any_origin())
        .with(warp::compression::gzip())
        .with(warp::log("image_processor"));

    // 启动服务器：组合 host:port 并解析为 SocketAddr 再传入 run（支持 ip 或 hostname）
    let addr: std::net::SocketAddr = format!("{}:{}", app_config.server.host, app_config.server.port).parse()?;
    warp::serve(routes)
        .run(addr)
        .await;

    Ok(())
}