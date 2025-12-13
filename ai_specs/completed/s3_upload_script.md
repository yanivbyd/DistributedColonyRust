# S3 Upload Script Specification

## Spec Header

**Status**: approved

---

## Overview

A bash script that periodically scans the `output/s3` directory, uploads files to S3 buckets (using directory names as bucket names), and deletes local files after successful upload. The script must handle AWS credentials for both localhost and AWS spot instance environments.

**What this adds**:
- Periodic S3 upload daemon script (`scripts/s3_upload_daemon.sh`)
- Localhost helper scripts (`scripts/local_run.sh`, `scripts/local_kill.sh`)
- Automatic credential detection (IAM role on EC2, AWS CLI credentials on localhost)
- File stability check to avoid uploading incomplete files
- Recursive directory scanning with bucket name mapping
- CDK integration for automatic daemon startup on EC2 instances

**What stays unchanged**:
- Existing file structure in `output/s3/`
- File naming conventions

## Requirements

### Functional Requirements

1. **Periodic Scanning**: Scan `output/s3/` at configurable intervals (default: 60 seconds)
2. **Bucket Mapping**: First-level directory under `output/s3/` = S3 bucket name, remaining path = S3 key
   - Example: `output/s3/distributed_colony/images_shots/file.png` → bucket: `distributed_colony`, key: `images_shots/file.png`
3. **File Stability Check**: Only upload files unchanged for configurable period (default: 1 minute) to avoid incomplete uploads
4. **Idempotency Check**: Before uploading, check if file already exists in S3 (same key). If exists and size matches, skip upload and delete local file to prevent duplicates when scans overlap
5. **Local Cleanup**: Delete local files only after successful S3 upload
6. **Dual Environment**: Works on localhost (AWS CLI credentials) and EC2 (IAM role)
7. **Error Handling**: Log errors but continue processing other files

### AWS Credentials Handling

**EC2 Detection**: Query `http://169.254.169.254/latest/meta-data/instance-id` to detect EC2 environment

**On EC2**: Use IAM instance role (automatic via AWS CLI). Validate with `aws sts get-caller-identity`

**On Localhost**: Use AWS CLI credentials from `~/.aws/credentials`. Validate before starting, exit with clear error if missing

## Implementation Details

### Script Structure

Main loop: (1) Detect environment, (2) Validate AWS access, (3) Scan `output/s3/` for directories, (4) For each file: check stability, check if already in S3, upload if missing, delete on success, (5) Sleep and repeat

### Key Functions

- `is_ec2_instance()`: Query instance metadata service
- `validate_aws_access()`: Test `aws sts get-caller-identity`
- `is_file_stable(file_path)`: Check mtime hasn't changed for `FILE_STABILITY_SECONDS` (default: 2)
- `file_exists_in_s3(bucket, key)`: Check if object exists using `aws s3 ls` or `aws s3api head-object`, compare size if exists
- `upload_file_to_s3(file, bucket, key)`: Use `aws s3 cp` with `--only-show-errors`
- `scan_and_upload()`: Main logic using `find` to discover files recursively

### File Stability Check

**Algorithm**: Compare file's last modification time (mtime) with current time. If the difference is greater than `FILE_STABILITY_SECONDS`, the file is considered stable and ready for upload. This approach avoids sleeping and allows immediate processing of files that haven't been modified recently.

**Implementation**: Use `stat -f %m` (macOS) or `stat -c %Y` (Linux) to get mtime. Calculate `current_time - mtime` and compare with `FILE_STABILITY_SECONDS` threshold. Use 1-second tolerance for filesystem granularity.

### Idempotency Check

**Potential problem**: If scan takes > 1 minute, next scan may find same file before it's deleted, causing duplicate uploads.

**Solution**: Before uploading, check if file already exists in S3:
1. Use `aws s3api head-object --bucket BUCKET --key KEY` to check existence
2. If exists, compare local file size with S3 object size (`ContentLength`)
3. If sizes match: Skip upload, delete local file (already uploaded)
4. If sizes differ or doesn't exist: Proceed with upload

**Implementation**: `aws s3api head-object` returns metadata including `ContentLength`. Compare with local file size using `stat -f %z` (macOS) or `stat -c %s` (Linux).

### File Discovery

Use `find output/s3 -type f` to recursively discover files. Extract bucket name (first directory component) and S3 key (remaining path).

### Logging

Use `echo "[$(date +'%Y-%m-%d %H:%M:%S')] message"` with levels: INFO (successful uploads), ERROR (failures), WARN (skipped files, deletion failures)

## Configuration

### Environment Variables

- `S3_UPLOAD_INTERVAL`: Scan interval seconds (default: 60)
- `S3_ROOT_DIR`: Root directory (default: `output/s3`)
- `AWS_REGION`: AWS region (default: `eu-west-1`)
- `AWS_PROFILE`: AWS CLI profile (localhost only, optional)
- `FILE_STABILITY_SECONDS`: Stability check duration (default: 2)

### Command-Line Arguments

```
-i, --interval SECONDS    Scan interval (default: 60)
-d, --directory DIR       Root directory (default: output/s3)
-r, --region REGION       AWS region (default: eu-west-1)
-p, --profile PROFILE     AWS CLI profile (localhost only)
-s, --stability SECONDS   Stability check duration (default: 2)
-h, --help                Show help
```

## IAM Permissions

EC2 instance role needs S3 permissions:
- `s3:PutObject`, `s3:PutObjectAcl` on `arn:aws:s3:::distributed-colony/*`
- `s3:GetObject`, `s3:HeadObject` on `arn:aws:s3:::distributed-colony/*` (for idempotency check)
- `s3:ListBucket` on `arn:aws:s3:::distributed-colony`

**CDK Integration**: 
- Add S3 permissions to `CDK/lib/spot-instances-stack.ts` instanceRole policy
- In CDK user data builder, add command to start the daemon on instance launch
- Configure daemon to use the same directory that the coordinator writes to (must match coordinator's output configuration)
- Ensure daemon runs in background and persists across reboots if needed

## Error Scenarios

- **AWS CLI not installed**: Exit with installation instructions
- **Credentials invalid (localhost)**: Exit with `aws configure` instructions
- **IAM permissions missing (EC2)**: Log error, continue (will fail on upload)
- **Upload failure**: Log error, continue (retry next scan)
- **File already in S3**: Skip upload, delete local file, log info
- **File still being written**: Skip file, log warning (retry next scan)
- **Bucket doesn't exist**: Log error, skip file

## Localhost Helper Scripts

### local_run.sh

Script to start the S3 upload daemon on localhost. Should:
- Check if daemon is already running (prevent duplicates)
- Start daemon in background with appropriate logging
- Store PID for later termination
- Use coordinator's output directory configuration

### local_kill.sh

Script to stop the S3 upload daemon on localhost. Should:
- Find running daemon process
- Gracefully terminate the daemon
- Clean up PID files if used

## Usage Examples

**Localhost**: 
- Start: `./scripts/local_run.sh`
- Stop: `./scripts/local_kill.sh`
- Direct: `./scripts/s3_upload_daemon.sh --interval 30`

**EC2**: Daemon is automatically started via CDK user data (see CDK Integration section)

## Dependencies

- AWS CLI (v2 recommended)
- bash, curl, find (standard tools)