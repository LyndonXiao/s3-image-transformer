# ImgLake Engine

ImgLake Engine is a high-performance image processing service that retrieves images from S3-compatible storage, processes them on-the-fly, and serves them with efficient caching.

## Features

- On-the-fly image processing (resizing, quality adjustment, format conversion)
- S3-compatible storage integration
- In-memory caching with configurable TTL
- RESTful API with query parameter-based transformations
- Detailed performance logging
- Health check and statistics endpoints

## Configuration

The service is configured through `config.yaml`:

```yaml
server:
  host: "0.0.0.0"        # Server host
  port: 6699            # Server port

s3:
  endpoint: "http://10.118.17.41:9100"  # S3 endpoint
  access_key: "MJF52PGvA3k7NOdfRYhl"    # Access key
  secret_key: "L1iVQ2RcPbyAEDv3Yogl45XOWGhwJKmNSCTuHn8d"  # Secret key
  region: ""            # Region (optional)
  use_path_style: true  # Use path-style URLs

cache:
  max_capacity_mb: 512  # Maximum cache capacity in MB
  time_to_live_sec: 3600  # Entry TTL in seconds
  time_to_idle_sec: 1800  # Entry TTI in seconds

image_processing:
  default_quality: 80   # Default JPEG quality
  max_width: 1920       # Maximum image width
  max_height: 1080      # Maximum image height
```

## Deployment

### Prerequisites

- Rust toolchain (1.56 or newer)
- S3-compatible storage (e.g., MinIO, AWS S3)
- OpenCV library installed on the system:
  - Ubuntu/Debian: `sudo apt-get install libopencv-dev clang cmake`
  - CentOS/RHEL: `sudo yum install opencv-devel clang cmake`
  - macOS: `brew install opencv`

### Building

```bash
# Development build
cargo build

# Release build
cargo build --release
```

### Running

```bash
# Run directly
cargo run

# Run release version
cargo run --release

# Or run the compiled binary
./target/release/img-lake-engine
```

The service will start on the configured host and port (default: http://0.0.0.0:6699).

## Usage

### Image Retrieval and Processing

Images are accessed directly through the root path with the S3 key as the path:

```
GET /{bucket}/{object_key}?{parameters}
```

Parameters:
- `width` - Target width in pixels
- `height` - Target height in pixels
- `quality` - JPEG quality (1-100)
- `format` - Output format (jpg, png, webp)

Examples:
```
# Get original image
GET /my-bucket/my-image.jpg

# Resize to 300px width
GET /my-bucket/my-image.jpg?width=300

# Resize to 300x200
GET /my-bucket/my-image.jpg?width=300&height=200

# Change quality to 50
GET /my-bucket/my-image.jpg?quality=50

# Convert to PNG
GET /my-bucket/my-image.jpg?format=png

# Combination of parameters
GET /my-bucket/my-image.jpg?width=300&height=200&quality=75&format=webp
```

### Health Check

```
GET /health
```

Returns "OK" if the service is running.

### Statistics

```
GET /stats
```

Returns cache statistics including hit rate, entry count, and memory usage.

### Clear Cache

```
POST /clear-cache
```

Clears all cached entries.

## Performance Monitoring

The service logs detailed timing information for each processing step:
- Cache check time
- S3 fetch time
- Image processing time
- Cache update time
- Overall request time

These logs help identify performance bottlenecks and optimization opportunities.

## Technical Details

### Image Processing Pipeline

1. Check if requested image variant exists in cache
2. If not cached, fetch original image from S3
3. Process image according to parameters:
   - Resize with aspect ratio preservation
   - Adjust quality
   - Convert format
4. Store processed image in cache
5. Return processed image

### Caching Strategy

- Uses Moka cache for high-performance in-memory caching
- Cache key is generated from image key and processing parameters
- Configurable size limit and TTL/TTI settings
- Weighted by image size in bytes

### S3 Integration

- Supports any S3-compatible storage
- Path-style bucket access
- Configurable endpoint and credentials

### Image Processing Library

- Uses OpenCV for high-performance image processing operations
- Implements optimized processing pipeline with early returns for unchanged images
- Uses linear interpolation for high-quality resizing operations