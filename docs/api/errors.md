# Error Handling and Status Codes

This document describes the error handling patterns and HTTP status codes used by the ESdiag API.

## HTTP Status Codes

The ESdiag API uses standard HTTP status codes to indicate the success or failure of requests.

### Success Codes

| Code | Description | Usage |
|------|-------------|-------|
| 200 OK | Request successful | All successful API responses |

### Client Error Codes

| Code | Description | Usage |
|------|-------------|-------|
| 400 Bad Request | Invalid request data | Malformed JSON, invalid file types, missing required fields |

### Server Error Codes

| Code | Description | Usage |
|------|-------------|-------|
| 500 Internal Server Error | Server-side processing error | System failures, external service errors, processing failures |

## Error Response Format

All error responses follow a consistent JSON structure:

```json
{
  "error": "Human-readable error message",
  "status": "error"
}
```

Some endpoints may include additional fields for context:

```json
{
  "status": "error",
  "error": "Detailed error description"
}
```

## Endpoint-Specific Errors

### POST `/upload`

#### 400 Bad Request Errors

**Invalid File Type**
```json
{
  "error": "Invalid file type. Only .zip files are allowed."
}
```
- **Cause**: Uploaded file does not have a `.zip` extension
- **Resolution**: Ensure the file has a `.zip` extension

**No Filename Provided**
```json
{
  "error": "No file name provided"
}
```
- **Cause**: File upload missing filename metadata
- **Resolution**: Ensure the file has a valid filename

**No File in Request**
```json
{
  "status": "error",
  "error": "No file part in the request"
}
```
- **Cause**: Request missing file data or incorrect field name
- **Resolution**: Ensure multipart form includes a field named "file"

#### 500 Internal Server Error

**Processing Failure**
```json
{
  "status": "error",
  "error": "Failed to process the upload"
}
```
- **Cause**: Internal processing pipeline failure
- **Resolution**: Retry the request; contact support if persists

**File Read Error**
```json
{
  "status": "error",
  "error": "Failed to read upload data: <specific error>"
}
```
- **Cause**: Unable to read uploaded file data
- **Resolution**: Verify file integrity and retry

### POST `/upload_service`

#### 400 Bad Request Errors

**Invalid URL**
```json
{
  "error": "Invalid URL: <specific error details>"
}
```
- **Cause**: Malformed URL in request payload
- **Resolution**: Verify URL format (must be valid HTTP/HTTPS)

**Token Setup Failure**
```json
{
  "error": "Failed to set token in URL"
}
```
- **Cause**: Unable to embed authentication token in URL
- **Resolution**: Verify token format and URL structure

#### 500 Internal Server Error

**Receiver Creation Failure**
```json
{
  "error": "Failed to create receiver: <specific error details>"
}
```
- **Cause**: Unable to establish connection to external service
- **Resolution**: Verify external service availability and credentials

**Exporter Creation Failure**
```json
{
  "error": "Failed to create exporter: <specific error details>"
}
```
- **Cause**: Internal exporter initialization failure
- **Resolution**: Check system configuration and retry

**Job Preparation Failure**
```json
{
  "error": "Failed to prepare job: <specific error details>"
}
```
- **Cause**: Job initialization or validation failure
- **Resolution**: Verify request parameters and system state

## Common Error Scenarios

### Authentication Errors

While not explicitly handled by the application (delegated to Google IAP), authentication issues may manifest as:

- **Missing Authentication Header**: Request rejected before reaching application
- **Invalid User Email**: Extracted user email is malformed or empty
- **Session Expired**: Authentication token expired

### File Size Limits

**Request Too Large**
- **Cause**: File exceeds 1 GiB limit
- **Resolution**: Reduce file size or split into smaller chunks
- **Note**: This is enforced at the HTTP layer, not application level

### System Resource Limits

**Queue Full**
- **Behavior**: Requests accepted but queued (not an error)
- **Status Response**: `"status": "busy"` with warning message
- **Resolution**: Wait for queue to process or retry later

### Network and Connectivity

**Connection Timeout**
- **Cause**: Network issues or slow file uploads
- **Resolution**: Retry with smaller files or better network connection

**Service Unavailable**
- **Cause**: ESdiag service not running or overloaded
- **Resolution**: Check service status and retry

## Error Handling Best Practices

### Client-Side Error Handling

1. **Always Check HTTP Status Codes**
   ```javascript
   if (!response.ok) {
     throw new Error(`HTTP ${response.status}: ${response.statusText}`);
   }
   ```

2. **Parse Error Messages**
   ```javascript
   try {
     const data = await response.json();
     console.error('API Error:', data.error);
   } catch (parseError) {
     console.error('Failed to parse error response');
   }
   ```

3. **Implement Retry Logic**
   ```javascript
   async function uploadWithRetry(file, maxRetries = 3) {
     for (let attempt = 1; attempt <= maxRetries; attempt++) {
       try {
         return await uploadFile(file);
       } catch (error) {
         if (attempt === maxRetries || error.status === 400) {
           throw error; // Don't retry client errors
         }
         await delay(Math.pow(2, attempt) * 1000);
       }
     }
   }
   ```

### Server-Side Logging

All errors are logged with appropriate levels:

- **Client Errors (400)**: INFO level with request details
- **Server Errors (500)**: ERROR level with full stack traces
- **Authentication Issues**: WARN level with anonymized details

### Debugging Information

Error messages include contextual information when available:

- **File Upload Errors**: Include filename and file size
- **URL Validation Errors**: Include sanitized URL information
- **Processing Errors**: Include job ID and processing stage

## Error Recovery Strategies

### Automatic Recovery

The system automatically handles:

- **Transient Network Errors**: Internal retry mechanisms
- **Resource Exhaustion**: Queue management and backpressure
- **Worker Failures**: Job restart and error reporting

### Manual Recovery

For persistent errors:

1. **Check System Status**: Use `/status` endpoint
2. **Verify File Integrity**: Ensure uploaded files are not corrupted
3. **Validate Credentials**: Confirm authentication tokens are valid
4. **Monitor Queue**: Check if system is overloaded

### Monitoring and Alerting

Operators should monitor:

- **Error Rate**: Percentage of failed requests
- **Queue Length**: Number of pending jobs
- **Processing Time**: Average job completion time
- **System Resources**: Memory and CPU usage

## Error Message Localization

Currently, all error messages are in English. For internationalization:

- Error codes could be added for programmatic handling
- Localized error messages could be provided based on Accept-Language header
- Client-side error translation could be implemented

## Rate Limiting and Throttling

While not explicitly implemented, natural rate limiting occurs through:

- **Queue Size Limits**: Maximum 10 concurrent jobs
- **File Size Limits**: Maximum 1 GiB per request
- **Processing Time**: Sequential job processing

## Security Considerations

Error messages are designed to:

- **Avoid Information Disclosure**: No sensitive system details
- **Prevent Enumeration**: Generic messages for authentication failures
- **Limit Attack Surface**: Minimal error details for external services

## Troubleshooting Guide

### Common Issues and Solutions

**"Invalid file type" Error**
- Check file extension is `.zip`
- Verify file is not corrupted
- Ensure proper multipart form encoding

**"Failed to process upload" Error**
- Check system resources and logs
- Verify file integrity
- Retry with smaller file if possible

**"Invalid URL" Error**
- Validate URL format (http/https)
- Check for special characters
- Ensure URL is accessible

**"Queue full" Warning**
- Monitor `/status` endpoint
- Wait for queue to clear
- Consider processing during off-peak hours

### Log Analysis

Error logs include:

- **Timestamp**: When error occurred
- **Request ID**: For tracing requests
- **User Context**: Sanitized user information
- **Error Context**: Relevant system state

### Performance Impact

Error handling has minimal performance impact:

- **Validation**: Fast client-side checks
- **Logging**: Asynchronous error reporting
- **Recovery**: Automatic cleanup of failed jobs

## Future Enhancements

Planned improvements to error handling:

- **Structured Error Codes**: Machine-readable error identification
- **Retry-After Headers**: Guidance for client retry timing
- **Partial Success Handling**: For batch operations
- **Enhanced Error Context**: More detailed debugging information