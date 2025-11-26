use anyhow::Result;
use opencv::{
    prelude::*,
    imgcodecs::{imdecode, imencode, ImreadModes},
    imgproc::{resize, InterpolationFlags},
    core::{Mat, Size, Vector},
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
    hash::{Hash, Hasher, DefaultHasher},
};

use crate::{
    s3_client::S3Client,
    cache::ImageCache,
};

#[derive(Debug, Deserialize, Clone)]
pub struct ImageProcessingConfig {
    pub default_quality: i32,
    pub max_width: i32,
    pub max_height: i32,
}

#[derive(Debug, Clone)]
pub struct ProcessingParams {
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub quality: Option<i32>,
    pub format: Option<String>,
}

// 实现 Hash trait 用于缓存键生成
impl std::hash::Hash for ProcessingParams {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.hash(state);
        self.height.hash(state);
        self.quality.hash(state);
        self.format.hash(state);
    }
}

#[derive(Debug, Clone)]
pub struct ImageProcessor {
    s3_client: S3Client,
    cache: ImageCache,
    config: ImageProcessingConfig,
}

impl ImageProcessor {
    pub fn new(s3_client: S3Client, cache: ImageCache, config: ImageProcessingConfig) -> Self {
        Self {
            s3_client,
            cache,
            config,
        }
    }

    pub async fn process_image_data(
        &self,
        image_data: Vec<u8>,
        params: &ProcessingParams,
    ) -> Result<(Vec<u8>, String)> {
        let start_time = SystemTime::now();
        println!("Starting image processing at {:?}", start_time);

        // For images without processing parameters, return original data directly
        if params.width.is_none() && params.height.is_none() && params.quality.is_none() && params.format.is_none() {
            let duration = start_time.elapsed().unwrap_or_default();
            println!("Processing completed (no changes) in {:?}", duration);
            return Ok((image_data, "image/jpeg".to_string()));
        }
        
        println!("Processing image with OpenCV: {:?}", params);
        let load_start = SystemTime::now();
        
        // Load image with OpenCV
        let img_buf = Vector::<u8>::from_iter(image_data.iter().copied());
        let mut img = imdecode(&img_buf, ImreadModes::IMREAD_ANYCOLOR.into())?;
        let load_duration = load_start.elapsed().unwrap_or_default();
        println!("Image loading took: {:?}", load_duration);

        let resize_start = SystemTime::now();

        // 调整尺寸
        if let (Some(width), Some(height)) = (params.width, params.height) {
            let target_width = width.min(self.config.max_width);
            let target_height = height.min(self.config.max_height);
            let mut resized_img = Mat::default();
            resize(
                &img,
                &mut resized_img,
                Size::new(target_width, target_height),
                0.0,
                0.0,
                InterpolationFlags::INTER_LINEAR.into(),
            )?;
            img = resized_img;
        } else if let Some(width) = params.width {
            let aspect_ratio = img.rows() as f64 / img.cols() as f64;
            let target_width = width.min(self.config.max_width);
            let target_height = (target_width as f64 * aspect_ratio) as i32;
            let mut resized_img = Mat::default();
            resize(
                &img,
                &mut resized_img,
                Size::new(target_width, target_height),
                0.0,
                0.0,
                InterpolationFlags::INTER_LINEAR.into(),
            )?;
            img = resized_img;
        } else if let Some(height) = params.height {
            let aspect_ratio = img.cols() as f64 / img.rows() as f64;
            let target_height = height.min(self.config.max_height);
            let target_width = (target_height as f64 * aspect_ratio) as i32;
            let mut resized_img = Mat::default();
            resize(
                &img,
                &mut resized_img,
                Size::new(target_width, target_height),
                0.0,
                0.0,
                InterpolationFlags::INTER_LINEAR.into(),
            )?;
            img = resized_img;
        }

        let resize_duration = resize_start.elapsed().unwrap_or_default();
        println!("Image resizing took: {:?}", resize_duration);

        // 确定输出格式和内容类型
        let (extension, content_type, quality_flag) = match params.format.as_deref().unwrap_or("jpg") {
            "png" => (".png", "image/png", 16), // ImwriteFlags::PNG_COMPRESSION equivalent
            "webp" => (".webp", "image/webp", 64), // ImwriteFlags::WEBP_QUALITY equivalent
            _ => (".jpg", "image/jpeg", 1), // ImwriteFlags::JPEG_QUALITY equivalent
        };

        // 编码图片
        let encode_start = SystemTime::now();
        let mut buf = Vector::new();
        let quality = params.quality.unwrap_or(self.config.default_quality);
        let params_vec = Vector::from_slice(&[quality_flag, quality]);
        imencode(extension, &img, &mut buf, &params_vec)?;
        let encoded_data = buf.to_vec();
        let encode_duration = encode_start.elapsed().unwrap_or_default();
        println!("Image encoding took: {:?}", encode_duration);

        let duration = start_time.elapsed().unwrap_or_default();
        println!("Processing completed (full pipeline) in {:?}", duration);

        Ok((encoded_data, content_type.to_string()))
    }

    pub async fn get_or_process_image(
        &self,
        image_key: String,
        params: ProcessingParams,
    ) -> Result<(Vec<u8>, String, String)> {
        let overall_start = SystemTime::now();
        
        // 使用更高效的缓存键生成方式
        let mut hasher = DefaultHasher::new();
        image_key.hash(&mut hasher);
        params.width.hash(&mut hasher);
        params.height.hash(&mut hasher);
        params.quality.hash(&mut hasher);
        if let Some(ref format) = params.format {
            format.hash(&mut hasher);
        }
        let cache_key = hasher.finish().to_string();
        
        // 检查缓存
        let cache_check_start = SystemTime::now();
        if let Some(cached_data) = self.cache.get(&cache_key).await {
            let cache_duration = cache_check_start.elapsed().unwrap_or_default();
            println!("Cache check took: {:?}", cache_duration);
            
            // 确定缓存数据的内容类型
            let content_type = self.determine_content_type(&params);
            let overall_duration = overall_start.elapsed().unwrap_or_default();
            println!("Request served from cache in {:?}", overall_duration);
            return Ok((cached_data, content_type, "cache".to_string()));
        }
        let cache_duration = cache_check_start.elapsed().unwrap_or_default();
        println!("Cache check took: {:?}", cache_duration);

        // 获取原始图片 (同时获取对象并检查是否存在)
        let s3_fetch_start = SystemTime::now();
        let original_data = match self.s3_client.get_object(&image_key).await {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Object '{}' does not exist in S3 or cannot be accessed: {}", image_key, e);
                return Err(anyhow::anyhow!("Failed to get original image {}: {}", image_key, e));
            }
        };
        let s3_duration = s3_fetch_start.elapsed().unwrap_or_default();
        println!("S3 fetch took: {:?}", s3_duration);

        // 处理图片
        let process_start = SystemTime::now();
        let (processed_data, content_type) = 
            self.process_image_data(original_data, &params).await?;
        let process_duration = process_start.elapsed().unwrap_or_default();
        println!("Image processing took: {:?}", process_duration);

        // 更新缓存
        let cache_update_start = SystemTime::now();
        self.cache.insert(cache_key, processed_data.clone()).await;
        let cache_update_duration = cache_update_start.elapsed().unwrap_or_default();
        println!("Cache update took: {:?}", cache_update_duration);

        let overall_duration = overall_start.elapsed().unwrap_or_default();
        println!("Request processed and cached in {:?}", overall_duration);

        Ok((processed_data, content_type, "newly_processed".to_string()))
    }
    
    pub fn get_cache_stats(&self) -> String {
        self.cache.get_stats().to_string()
    }

    // 新增：清空缓存（供 /clear-cache 路由调用）
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
    }

    // 新增：确定内容类型的方法
    fn determine_content_type(&self, params: &ProcessingParams) -> String {
        match params.format.as_deref().unwrap_or("jpg") {
            "png" => "image/png".to_string(),
            "webp" => "image/webp".to_string(),
            _ => "image/jpeg".to_string(),
        }
    }
}

pub fn parse_query_params(params: HashMap<String, String>) -> ProcessingParams {
    ProcessingParams {
        width: params.get("width").and_then(|w| w.parse().ok()),
        height: params.get("height").and_then(|h| h.parse().ok()),
        quality: params.get("quality")
            .and_then(|q| q.parse().ok())
            .map(|q: i32| q.clamp(1, 100)),
        format: params.get("format").cloned(),
    }
}