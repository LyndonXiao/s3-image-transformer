use anyhow::Result;
use aws_sdk_s3::{Client, primitives::ByteStream};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub use_path_style: bool,
}

#[derive(Debug, Clone)]
pub struct S3Client {
    pub client: Arc<Client>,
    pub config: S3Config,
}

impl S3Client {
    pub async fn new(config: S3Config) -> Result<Self> {
        // 使用从 aws-sdk-s3 传递来的 aws_types / aws_credential_types 版本
        use aws_types::region::Region;
        use aws_credential_types::Credentials;

        let region = if config.region.is_empty() {
            Region::new("us-east-1")
        } else {
            Region::new(config.region.clone())
        };

        let credentials = Credentials::new(
            config.access_key.clone(),
            config.secret_key.clone(),
            None,
            None,
            "static",
        );

        let mut builder = aws_sdk_s3::config::Builder::new()
            .region(region)
            .credentials_provider(credentials)
            .force_path_style(config.use_path_style);

        if !config.endpoint.is_empty() {
            builder = builder.endpoint_url(&config.endpoint);
        }

        let s3_config = builder.build();
        let client = Client::from_conf(s3_config);

        Ok(Self {
            client: Arc::new(client),
            config,
        })
    }

    pub async fn get_object(&self, key: &str) -> Result<Vec<u8>> {
        // Parse the key to extract bucket and object key
        // Expected format: bucket_name/object_key
        let parts: Vec<&str> = key.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("Invalid key format. Expected 'bucket_name/object_key', got '{}'", key));
        }
        
        let bucket = parts[0];
        let object_key = parts[1];
        
        println!("Attempting to fetch object with key: '{}' from bucket: '{}'", object_key, bucket);
        
        let response = self.client
            .get_object()
            .bucket(bucket)
            .key(object_key)
            .send()
            .await;

        match response {
            Ok(resp) => {
                let data = resp.body.collect().await?;
                let data_vec = data.into_bytes().to_vec();
                println!("Successfully fetched object '{}/{}', size: {} bytes", bucket, object_key, data_vec.len());
                Ok(data_vec)
            }
            Err(e) => {
                eprintln!("Failed to fetch object '{}/{}': {}", bucket, object_key, e);
                // Let's also log the specific type of error
                eprintln!("Error type: {:?}", e);
                Err(anyhow::anyhow!("S3 get_object failed for key '{}/{}': {}", bucket, object_key, e))
            }
        }
    }

    pub async fn put_object(&self, key: &str, data: Vec<u8>, content_type: &str) -> Result<()> {
        // Parse the key to extract bucket and object key
        // Expected format: bucket_name/object_key
        let parts: Vec<&str> = key.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!("Invalid key format. Expected 'bucket_name/object_key', got '{}'", key));
        }
        
        let bucket = parts[0];
        let object_key = parts[1];
        
        let byte_stream = ByteStream::from(data);
        
        self.client
            .put_object()
            .bucket(bucket)
            .key(object_key)
            .body(byte_stream)
            .content_type(content_type)
            .send()
            .await?;

        Ok(())
    }

    pub async fn object_exists(&self, key: &str) -> bool {
        // Parse the key to extract bucket and object key
        // Expected format: bucket_name/object_key
        let parts: Vec<&str> = key.splitn(2, '/').collect();
        if parts.len() != 2 {
            return false;
        }
        
        let bucket = parts[0];
        let object_key = parts[1];
        
        self.client
            .head_object()
            .bucket(bucket)
            .key(object_key)
            .send()
            .await
            .is_ok()
    }

    pub async fn ensure_bucket_exists(&self) -> Result<()> {
        // This function is no longer applicable since we don't have a fixed bucket
        Ok(())
    }
    
    pub async fn list_objects(&self, _prefix: &str) -> Result<Vec<String>> {
        // This function is no longer applicable since we don't have a fixed bucket
        Ok(Vec::new())
    }
}