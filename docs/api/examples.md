# API Usage Examples

This document provides practical examples of how to use the ESdiag API endpoints.

## Authentication

All requests require the `X-Goog-Authenticated-User-Email` header, which is typically set automatically by Google's Identity-Aware Proxy (IAP).

```bash
# Example header (usually set by IAP)
X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com
```

## GET `/` - Main Interface

### Request
```bash
curl -X GET http://localhost:3000/ \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com"
```

### Response
```html
<!DOCTYPE html>
<html>
<head>
    <title>ESdiag</title>
</head>
<body>
    <!-- Main application interface -->
</body>
</html>
```

## POST `/upload` - Upload Diagnostic Bundle

### Request
```bash
curl -X POST http://localhost:3000/upload \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com" \
  -F "file=@diagnostic-bundle.zip"
```

### Successful Response
```json
{
  "status": "processing",
  "message": "Received upload: diagnostic-bundle.zip (15728640 bytes)"
}
```

### Error Response - Invalid File Type
```json
{
  "error": "Invalid file type. Only .zip files are allowed."
}
```

### Error Response - No File Provided
```json
{
  "status": "error",
  "error": "No file part in the request"
}
```

## GET `/status` - Check Processing Status

### Request
```bash
curl -X GET http://localhost:3000/status \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com"
```

### Response - System Ready
```json
{
  "status": "ready",
  "exporter": "elasticsearch-exporter",
  "kibana": "https://kibana.example.com",
  "user": "user@example.com",
  "current": null,
  "queue": {
    "size": 0
  },
  "history": [
    {
      "id": "job-123",
      "status": "completed",
      "created_at": "2024-01-15T10:30:00Z",
      "updated_at": "2024-01-15T10:35:00Z",
      "identifiers": {
        "account": "customer-abc",
        "case_number": 12345,
        "filename": "diagnostic-bundle.zip",
        "opportunity": "opp-456",
        "user": "user@example.com"
      },
      "error": null
    }
  ]
}
```

### Response - System Processing
```json
{
  "status": "processing",
  "progress": "Processing diagnostic...",
  "kibana": "https://kibana.example.com",
  "user": "user@example.com",
  "current": {
    "id": "job-124",
    "status": "processing",
    "created_at": "2024-01-15T11:00:00Z",
    "updated_at": "2024-01-15T11:02:00Z",
    "identifiers": {
      "account": null,
      "case_number": null,
      "filename": "new-diagnostic.zip",
      "opportunity": null,
      "user": "user@example.com"
    },
    "error": null
  },
  "queue": {
    "size": 2
  },
  "history": [
    {
      "id": "job-123",
      "status": "completed",
      "created_at": "2024-01-15T10:30:00Z",
      "updated_at": "2024-01-15T10:35:00Z",
      "identifiers": {
        "account": "customer-abc",
        "case_number": 12345,
        "filename": "diagnostic-bundle.zip",
        "opportunity": "opp-456",
        "user": "user@example.com"
      },
      "error": null
    }
  ]
}
```

### Response - System Busy
```json
{
  "status": "busy",
  "warning": "Too many jobs in queue",
  "kibana": "https://kibana.example.com",
  "user": "user@example.com",
  "current": {
    "id": "job-125",
    "status": "processing",
    "created_at": "2024-01-15T11:15:00Z",
    "updated_at": "2024-01-15T11:17:00Z",
    "identifiers": {
      "account": "customer-xyz",
      "case_number": 67890,
      "filename": "large-diagnostic.zip",
      "opportunity": "opp-789",
      "user": "user@example.com"
    },
    "error": null
  },
  "queue": {
    "size": 12
  },
  "history": []
}
```

## POST `/upload_service` - Remote Service Processing

### Request
```bash
curl -X POST http://localhost:3000/upload_service \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com" \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "account": "customer-123",
      "case_number": 98765,
      "filename": "remote-diagnostic.zip",
      "opportunity": "opp-abc",
      "user": "user@example.com"
    },
    "token": "your-elasticsearch-token",
    "url": "https://elasticsearch-service.example.com/_diagnostic/upload"
  }'
```

### Successful Response
```json
{
  "status": "processing",
  "job_id": "job-remote-456",
  "queue_size": 3
}
```

### Error Response - Invalid URL
```json
{
  "error": "Invalid URL: relative URL without a base"
}
```

### Error Response - Token Setup Failure
```json
{
  "error": "Failed to set token in URL"
}
```

### Error Response - Service Creation Failure
```json
{
  "error": "Failed to create receiver: Connection refused"
}
```

## Complete Workflow Examples

### Example 1: Basic File Upload Workflow

```bash
# 1. Check system status
curl -X GET http://localhost:3000/status \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com"

# 2. Upload diagnostic bundle
curl -X POST http://localhost:3000/upload \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com" \
  -F "file=@diagnostic-bundle.zip"

# 3. Monitor processing status
curl -X GET http://localhost:3000/status \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com"
```

### Example 2: Remote Service Processing Workflow

```bash
# 1. Check system status
curl -X GET http://localhost:3000/status \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com"

# 2. Initiate remote processing
curl -X POST http://localhost:3000/upload_service \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com" \
  -H "Content-Type: application/json" \
  -d '{
    "metadata": {
      "account": "acme-corp",
      "case_number": 12345,
      "filename": "cluster-diagnostic.zip",
      "opportunity": "enterprise-deal",
      "user": "support@acme-corp.com"
    },
    "token": "esb_token_here",
    "url": "https://my-cluster.elasticsearch.com/_diagnostic/upload"
  }'

# 3. Monitor processing status
curl -X GET http://localhost:3000/status \
  -H "X-Goog-Authenticated-User-Email: accounts.google.com:user@example.com"
```

## JavaScript/TypeScript Examples

### Upload File with Fetch API

```javascript
async function uploadDiagnostic(file) {
  const formData = new FormData();
  formData.append('file', file);

  try {
    const response = await fetch('/upload', {
      method: 'POST',
      body: formData,
      headers: {
        'X-Goog-Authenticated-User-Email': 'accounts.google.com:user@example.com'
      }
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const result = await response.json();
    console.log('Upload successful:', result);
    return result;
  } catch (error) {
    console.error('Upload failed:', error);
    throw error;
  }
}
```

### Check Status with Fetch API

```javascript
async function checkStatus() {
  try {
    const response = await fetch('/status', {
      headers: {
        'X-Goog-Authenticated-User-Email': 'accounts.google.com:user@example.com'
      }
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const status = await response.json();
    console.log('Current status:', status);
    return status;
  } catch (error) {
    console.error('Status check failed:', error);
    throw error;
  }
}
```

### Remote Service Processing with Fetch API

```javascript
async function processRemoteDiagnostic(metadata, token, url) {
  const payload = {
    metadata,
    token,
    url
  };

  try {
    const response = await fetch('/upload_service', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Goog-Authenticated-User-Email': 'accounts.google.com:user@example.com'
      },
      body: JSON.stringify(payload)
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    const result = await response.json();
    console.log('Remote processing initiated:', result);
    return result;
  } catch (error) {
    console.error('Remote processing failed:', error);
    throw error;
  }
}
```

## Python Examples

### Upload File with Requests

```python
import requests

def upload_diagnostic(file_path, base_url="http://localhost:3000"):
    headers = {
        'X-Goog-Authenticated-User-Email': 'accounts.google.com:user@example.com'
    }

    with open(file_path, 'rb') as f:
        files = {'file': f}
        response = requests.post(f"{base_url}/upload", headers=headers, files=files)

    response.raise_for_status()
    return response.json()

# Usage
try:
    result = upload_diagnostic("diagnostic-bundle.zip")
    print("Upload successful:", result)
except requests.exceptions.RequestException as e:
    print("Upload failed:", e)
```

### Check Status with Requests

```python
import requests

def check_status(base_url="http://localhost:3000"):
    headers = {
        'X-Goog-Authenticated-User-Email': 'accounts.google.com:user@example.com'
    }

    response = requests.get(f"{base_url}/status", headers=headers)
    response.raise_for_status()
    return response.json()

# Usage
try:
    status = check_status()
    print("Current status:", status)
except requests.exceptions.RequestException as e:
    print("Status check failed:", e)
```

### Remote Service Processing with Requests

```python
import requests

def process_remote_diagnostic(metadata, token, url, base_url="http://localhost:3000"):
    headers = {
        'Content-Type': 'application/json',
        'X-Goog-Authenticated-User-Email': 'accounts.google.com:user@example.com'
    }

    payload = {
        'metadata': metadata,
        'token': token,
        'url': url
    }

    response = requests.post(f"{base_url}/upload_service", headers=headers, json=payload)
    response.raise_for_status()
    return response.json()

# Usage
metadata = {
    'account': 'customer-123',
    'case_number': 98765,
    'filename': 'remote-diagnostic.zip',
    'opportunity': 'opp-abc',
    'user': 'user@example.com'
}

try:
    result = process_remote_diagnostic(
        metadata=metadata,
        token='your-elasticsearch-token',
        url='https://elasticsearch-service.example.com/_diagnostic/upload'
    )
    print("Remote processing initiated:", result)
except requests.exceptions.RequestException as e:
    print("Remote processing failed:", e)
```

## Error Handling Best Practices

### Retry Logic Example

```javascript
async function uploadWithRetry(file, maxRetries = 3) {
  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      const result = await uploadDiagnostic(file);
      return result;
    } catch (error) {
      console.log(`Upload attempt ${attempt} failed:`, error.message);

      if (attempt === maxRetries) {
        throw new Error(`Upload failed after ${maxRetries} attempts`);
      }

      // Wait before retrying (exponential backoff)
      const delay = Math.pow(2, attempt) * 1000;
      await new Promise(resolve => setTimeout(resolve, delay));
    }
  }
}
```

### Status Polling Example

```javascript
async function pollStatus(jobId, timeout = 300000) { // 5 minutes
  const startTime = Date.now();

  while (Date.now() - startTime < timeout) {
    try {
      const status = await checkStatus();

      if (status.status === 'ready') {
        // Check if our job is in history
        const completedJob = status.history.find(job => job.id === jobId);
        if (completedJob) {
          return completedJob;
        }
      }

      // Wait before next poll
      await new Promise(resolve => setTimeout(resolve, 5000));
    } catch (error) {
      console.error('Status poll failed:', error);
      await new Promise(resolve => setTimeout(resolve, 10000));
    }
  }

  throw new Error('Job status polling timeout');
}
```

## Notes

- Replace `localhost:3000` with your actual server address and port
- Authentication headers are typically handled automatically by Google IAP in production
- File uploads are limited to 1 GiB
- Only ZIP files are accepted for upload
- The system processes jobs sequentially, so large files may take time to process
- Job history is filtered per user for privacy and security
