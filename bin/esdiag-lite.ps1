# ESDiag Lite is a collection-only Elasticsearch diagnostic utility for
# Windows PowerShell 5.1 and newer. It saves raw API responses for later
# processing with `esdiag process` and can optionally forward a completed ZIP
# archive to Elastic Upload Service.

param(
  [Parameter(Position = 0)]
  [string]$Command,
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$RemainingArguments
)

$script:WaitSeconds = if ($env:WAIT_SECONDS) { [int]$env:WAIT_SECONDS } else { 60 }
$script:CollectionCount = if ($env:COLLECTION_COUNT) { [int]$env:COLLECTION_COUNT } else { 5 }
$script:ArchiveFormat = 'zip'
$script:CommandName = ''
$script:AuthMode = ''
$script:Directory = ''
$script:UploadHost = if ($env:UPLOAD_HOST) { $env:UPLOAD_HOST } else { 'https://upload.elastic.co' }
$script:UploadId = if ($env:UPLOAD_ID) { $env:UPLOAD_ID } else { '' }
$script:UploadFile = ''
$script:UploadRequested = $false
$script:ClusterVersion = ''
$script:EsMajor = [Int64]0
$script:EsMinor = [Int64]0
$script:EsPatch = [Int64]0

function Write-Log {
  param(
    [string]$Level,
    [string]$Message
  )

  $timestamp = [DateTime]::UtcNow.ToString('yyyy-MM-dd HH:mm:ss')
  $line = "[$timestamp $Level esdiag-lite] $Message"
  if ($Level -eq 'Error') {
    [Console]::Error.WriteLine($line)
  }
  else {
    Write-Host $line
  }
}

function Show-Help {
  @"
Usage: esdiag-lite.ps1 <COMMAND> [OPTIONS]

Commands:
  watch                    Collect diagnostics periodically using WAIT_SECONDS and COLLECTION_COUNT.
  collect                  Collect a single diagnostic immediately.
  upload <filename> [id]   Upload an existing ZIP archive; uses UPLOAD_ID when id is omitted.

Options:
  --archive=zip|none       Output format; zip is the default and none preserves the directory.
  --upload=UPLOAD_ID       Upload the generated ZIP archive to an Elastic Upload Service id.

Environment:
  ELASTIC_ES_URL           Elasticsearch endpoint URL.
  ELASTIC_ES_API_KEY       Encoded Elasticsearch API key; takes precedence over basic authentication.
  ELASTIC_ES_USERNAME      Username for HTTP basic authentication.
  ELASTIC_ES_PASSWORD      Password for HTTP basic authentication.
  UPLOAD_HOST              Elastic Upload Service base URL; defaults to https://upload.elastic.co.
  UPLOAD_ID                Elastic Upload Service id used by upload when [id] is omitted.

ESDiag Lite collects raw diagnostic API responses and can forward its ZIP output.
Process its ZIP or directory output with esdiag process.
"@ | Write-Host
}

function Parse-Arguments {
  if ([string]::IsNullOrWhiteSpace($Command)) {
    Write-Log Error 'missing or unknown command'
    Show-Help
    return $false
  }

  switch ($Command) {
    'help' { $script:CommandName = 'help'; return $true }
    '--help' { $script:CommandName = 'help'; return $true }
    '-h' { $script:CommandName = 'help'; return $true }
    'collect' { $script:CommandName = 'collect' }
    'watch' { $script:CommandName = 'watch' }
    'upload' {
      $script:CommandName = 'upload'
      if ($RemainingArguments.Count -lt 1 -or $RemainingArguments.Count -gt 2) {
        Write-Log Error 'upload requires a filename and accepts an optional upload id'
        Show-Help
        return $false
      }
      $script:UploadFile = $RemainingArguments[0]
      $script:UploadRequested = $true
      if ($RemainingArguments.Count -eq 2) {
        $script:UploadId = $RemainingArguments[1]
      }
      return $true
    }
    default {
      Write-Log Error 'missing or unknown command'
      Show-Help
      return $false
    }
  }

  foreach ($argument in $RemainingArguments) {
    if ($argument -eq '--archive=zip') {
      $script:ArchiveFormat = 'zip'
    }
    elseif ($argument -eq '--archive=none') {
      $script:ArchiveFormat = 'none'
    }
    elseif ($argument.StartsWith('--archive=')) {
      Write-Log Error 'archive must be zip or none'
      Show-Help
      return $false
    }
    elseif ($argument.StartsWith('--upload=')) {
      $script:UploadId = $argument.Substring('--upload='.Length)
      if ([string]::IsNullOrWhiteSpace($script:UploadId)) {
        Write-Log Error 'upload id must not be empty'
        return $false
      }
      $script:UploadRequested = $true
    }
    else {
      Write-Log Error "unknown argument: $argument"
      Show-Help
      return $false
    }
  }
  return $true
}

function Test-Configuration {
  if ([string]::IsNullOrWhiteSpace($env:ELASTIC_ES_URL)) {
    Write-Log Error 'ELASTIC_ES_URL must be set'
    return $false
  }
  if (-not [string]::IsNullOrWhiteSpace($env:ELASTIC_ES_API_KEY)) {
    $script:AuthMode = 'api_key'
    return $true
  }
  if (-not [string]::IsNullOrWhiteSpace($env:ELASTIC_ES_USERNAME) -and -not [string]::IsNullOrWhiteSpace($env:ELASTIC_ES_PASSWORD)) {
    $script:AuthMode = 'basic'
    return $true
  }
  Write-Log Error 'a complete ELASTIC_ES_API_KEY or ELASTIC_ES_USERNAME/ELASTIC_ES_PASSWORD pair is required'
  return $false
}

function Test-UploadConfiguration {
  if ([string]::IsNullOrWhiteSpace($script:UploadId)) {
    Write-Log Error 'upload id must be provided as [id] or UPLOAD_ID'
    return $false
  }
  if (-not (Test-Path -LiteralPath $script:UploadFile -PathType Leaf)) {
    Write-Log Error "upload file does not exist: $($script:UploadFile)"
    return $false
  }
  return $true
}

function Test-Dependencies {
  if ($script:CommandName -ne 'upload' -and $script:ArchiveFormat -eq 'zip' -and -not (Get-Command Compress-Archive -ErrorAction SilentlyContinue)) {
    [Console]::Error.WriteLine('No ZIP archive support found, run with --archive=none to skip archive creation')
    return $false
  }
  if ($script:UploadRequested -and -not (Get-Command Get-FileHash -ErrorAction SilentlyContinue)) {
    Write-Log Error 'missing required PowerShell command Get-FileHash for uploads'
    return $false
  }
  return $true
}

function Test-VersionAtLeast {
  param([Int64]$Major, [Int64]$Minor, [Int64]$Patch)
  return $script:EsMajor -gt $Major -or ($script:EsMajor -eq $Major -and ($script:EsMinor -gt $Minor -or ($script:EsMinor -eq $Minor -and $script:EsPatch -ge $Patch)))
}

function Test-VersionGreaterThan {
  param([Int64]$Major, [Int64]$Minor, [Int64]$Patch)
  return $script:EsMajor -gt $Major -or ($script:EsMajor -eq $Major -and ($script:EsMinor -gt $Minor -or ($script:EsMinor -eq $Minor -and $script:EsPatch -gt $Patch)))
}

function Test-VersionAtMost {
  param([Int64]$Major, [Int64]$Minor, [Int64]$Patch)
  return $script:EsMajor -lt $Major -or ($script:EsMajor -eq $Major -and ($script:EsMinor -lt $Minor -or ($script:EsMinor -eq $Minor -and $script:EsPatch -le $Patch)))
}

function Test-VersionLessThan {
  param([Int64]$Major, [Int64]$Minor, [Int64]$Patch)
  return $script:EsMajor -lt $Major -or ($script:EsMajor -eq $Major -and ($script:EsMinor -lt $Minor -or ($script:EsMinor -eq $Minor -and $script:EsPatch -lt $Patch)))
}

function Read-ClusterVersion {
  try {
    $root = Get-Content -LiteralPath (Join-Path $script:Directory 'version.json') -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    $version = [string]$root.version.number
  }
  catch {
    Write-Log Error "could not extract version.number from version.json: $($_.Exception.Message)"
    return $false
  }
  if ($version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$') {
    Write-Log Error 'version.number is not a valid Elasticsearch version'
    return $false
  }
  $script:ClusterVersion = $version
  $numericVersion = ($version -split '[-+]', 2)[0].Split('.')
  $script:EsMajor = [Int64]$numericVersion[0]
  $script:EsMinor = [Int64]$numericVersion[1]
  $script:EsPatch = [Int64]$numericVersion[2]
  return $true
}

function Save-Manifest {
  $manifest = [ordered]@{
    mode = 'minimum'
    product = 'elasticsearch'
    flags = 'None'
    diagnostic = $null
    type = 'elasticsearch_diagnostic'
    runner = 'esdiag-lite'
    version = $script:ClusterVersion
    timestamp = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
  }
  $manifest | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $script:Directory 'diagnostic_manifest.json') -Encoding UTF8
}

function Get-RequestHeaders {
  $headers = @{ 'X-Management-Request' = 'true' }
  if ($script:AuthMode -eq 'api_key') {
    $headers.Authorization = "ApiKey $($env:ELASTIC_ES_API_KEY)"
  }
  else {
    $credentials = "$($env:ELASTIC_ES_USERNAME):$($env:ELASTIC_ES_PASSWORD)"
    $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($credentials))
    $headers.Authorization = "Basic $encoded"
  }
  return $headers
}

function Invoke-Api {
  param([string]$Api, [string]$Output)

  $outputPath = Join-Path $script:Directory $Output
  [IO.Directory]::CreateDirectory((Split-Path -Parent $outputPath)) | Out-Null
  $requestUrl = "$($env:ELASTIC_ES_URL.TrimEnd('/'))$Api"
  Write-Log Info "saving $Api to $outputPath"
  try {
    Invoke-WebRequest -UseBasicParsing -Uri $requestUrl -Headers (Get-RequestHeaders) -OutFile $outputPath -ErrorAction Stop | Out-Null
    return $true
  }
  catch {
    Write-Log Error "failed to save $Api"
    return $false
  }
}

function Skip-Api {
  param([string]$Name)
  Write-Log Info "skipping $Name because it is unsupported on Elasticsearch $($script:ClusterVersion)"
}

# BEGIN GENERATED LITE APIS
# This region is generated by `cargo run --bin esdiag-lite-generate`. Do not edit.

function Get-ApiAlias {
  if ((Test-VersionAtLeast 0 9 0)) {
    return Invoke-Api '/_alias?human' 'alias.json'
  }
  else {
    Skip-Api 'alias'
    return $true
  }
}

function Get-ApiClusterPendingTasks {
  if ((Test-VersionAtLeast 0 9 0)) {
    return Invoke-Api '/_cluster/pending_tasks?human' 'cluster_pending_tasks.json'
  }
  else {
    Skip-Api 'cluster_pending_tasks'
    return $true
  }
}

function Get-ApiClusterSettingsDefaults {
  if ((Test-VersionAtLeast 6 4 0)) {
    return Invoke-Api '/_cluster/settings?include_defaults&flat_settings' 'cluster_settings_defaults.json'
  }
  else {
    Skip-Api 'cluster_settings_defaults'
    return $true
  }
}

function Get-ApiDataStream {
  if ((Test-VersionAtLeast 7 11 0)) {
    return Invoke-Api '/_data_stream?expand_wildcards=all' 'commercial/data_stream.json'
  }
  elseif ((Test-VersionAtLeast 7 9 0) -and (Test-VersionLessThan 7 11 0)) {
    return Invoke-Api '/_data_stream' 'commercial/data_stream.json'
  }
  else {
    Skip-Api 'data_stream'
    return $true
  }
}

function Get-ApiIlmExplain {
  if ((Test-VersionAtLeast 6 6 0) -and (Test-VersionLessThan 7 7 0)) {
    return Invoke-Api '/*/_ilm/explain?human' 'commercial/ilm_explain.json'
  }
  elseif ((Test-VersionAtLeast 7 7 0)) {
    return Invoke-Api '/*/_ilm/explain?human&expand_wildcards=all' 'commercial/ilm_explain.json'
  }
  else {
    Skip-Api 'ilm_explain'
    return $true
  }
}

function Get-ApiIlmPolicies {
  if ((Test-VersionAtLeast 6 6 0)) {
    return Invoke-Api '/_ilm/policy?human' 'commercial/ilm_policies.json'
  }
  else {
    Skip-Api 'ilm_policies'
    return $true
  }
}

function Get-ApiIndicesSettings {
  if ((Test-VersionAtLeast 0 9 0) -and (Test-VersionLessThan 7 7 0)) {
    return Invoke-Api '/_settings?human' 'indices_settings.json'
  }
  elseif ((Test-VersionAtLeast 7 7 0)) {
    return Invoke-Api '/_settings?human&expand_wildcards=all' 'indices_settings.json'
  }
  else {
    Skip-Api 'indices_settings'
    return $true
  }
}

function Get-ApiIndicesStats {
  if ((Test-VersionAtLeast 0 9 0) -and (Test-VersionLessThan 7 7 0)) {
    return Invoke-Api '/_stats?level=shards&human' 'indices_stats.json'
  }
  elseif ((Test-VersionAtLeast 7 7 0)) {
    return Invoke-Api '/_stats?level=shards&human&expand_wildcards=all' 'indices_stats.json'
  }
  else {
    Skip-Api 'indices_stats'
    return $true
  }
}

function Get-ApiLicenses {
  if ((Test-VersionAtLeast 1 0 0) -and (Test-VersionLessThan 2 0 0)) {
    return Invoke-Api '/_licenses' 'licenses.json'
  }
  elseif ((Test-VersionAtLeast 2 0 0) -and (Test-VersionLessThan 7 6 0)) {
    return Invoke-Api '/_license' 'licenses.json'
  }
  elseif ((Test-VersionAtLeast 7 6 0) -and (Test-VersionLessThan 8 0 0)) {
    return Invoke-Api '/_license?accept_enterprise=true' 'licenses.json'
  }
  elseif ((Test-VersionAtLeast 8 0 0)) {
    return Invoke-Api '/_license' 'licenses.json'
  }
  else {
    Skip-Api 'licenses'
    return $true
  }
}

function Get-ApiNodes {
  if ((Test-VersionAtLeast 0 9 0)) {
    return Invoke-Api '/_nodes?human' 'nodes.json'
  }
  else {
    Skip-Api 'nodes'
    return $true
  }
}

function Get-ApiNodesStats {
  if ((Test-VersionAtLeast 0 9 0)) {
    return Invoke-Api '/_nodes/stats?human' 'nodes_stats.json'
  }
  else {
    Skip-Api 'nodes_stats'
    return $true
  }
}

function Get-ApiSearchableSnapshotsCacheStats {
  if ((Test-VersionAtLeast 7 13 0)) {
    return Invoke-Api '/_searchable_snapshots/cache/stats' 'commercial/searchable_snapshots_cache_stats.json'
  }
  else {
    Skip-Api 'searchable_snapshots_cache_stats'
    return $true
  }
}

function Get-ApiSlmPolicies {
  if ((Test-VersionAtLeast 7 4 0)) {
    return Invoke-Api '/_slm/policy?human' 'commercial/slm_policies.json'
  }
  else {
    Skip-Api 'slm_policies'
    return $true
  }
}

function Get-ApiTasks {
  if ((Test-VersionAtLeast 2 0 0)) {
    return Invoke-Api '/_tasks?human&detailed=true' 'tasks.json'
  }
  else {
    Skip-Api 'tasks'
    return $true
  }
}

function Get-ApiVersion {
  return Invoke-Api '/' 'version.json'
}

function Invoke-LiteApis {
  $failed = $false
  if (-not (Get-ApiAlias)) { $failed = $true }
  if (-not (Get-ApiClusterPendingTasks)) { $failed = $true }
  if (-not (Get-ApiClusterSettingsDefaults)) { $failed = $true }
  if (-not (Get-ApiDataStream)) { $failed = $true }
  if (-not (Get-ApiIlmExplain)) { $failed = $true }
  if (-not (Get-ApiIlmPolicies)) { $failed = $true }
  if (-not (Get-ApiIndicesSettings)) { $failed = $true }
  if (-not (Get-ApiIndicesStats)) { $failed = $true }
  if (-not (Get-ApiLicenses)) { $failed = $true }
  if (-not (Get-ApiNodes)) { $failed = $true }
  if (-not (Get-ApiNodesStats)) { $failed = $true }
  if (-not (Get-ApiSearchableSnapshotsCacheStats)) { $failed = $true }
  if (-not (Get-ApiSlmPolicies)) { $failed = $true }
  if (-not (Get-ApiTasks)) { $failed = $true }
  return (-not $failed)
}
# END GENERATED LITE APIS

function Complete-Archive {
  if ($script:ArchiveFormat -eq 'none') {
    Write-Log Info "completed directory $($script:Directory)"
    return $true
  }
  $archivePath = "$($script:Directory).zip"
  try {
    $items = Get-ChildItem -LiteralPath $script:Directory -Force -ErrorAction Stop | ForEach-Object { $_.FullName }
    Compress-Archive -Path $items -DestinationPath $archivePath -Force -ErrorAction Stop
    Remove-Item -LiteralPath $script:Directory -Recurse -Force -ErrorAction Stop
    $script:UploadFile = $archivePath
    Write-Log Info "completed archive $archivePath"
    return $true
  }
  catch {
    Write-Log Error "failed to create archive $archivePath; preserving $($script:Directory)"
    return $false
  }
}

function Get-FileDigest {
  param([string]$Path)
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
}

function Get-UploadId {
  $segments = $script:UploadId.TrimEnd('/') -split '/'
  return $segments[$segments.Count - 1]
}

function New-UploadParts {
  param([string]$SourcePath, [string]$DestinationDirectory)

  $partSize = 50000000
  $buffer = New-Object byte[] 1048576
  $parts = New-Object System.Collections.Generic.List[string]
  $source = [IO.File]::OpenRead($SourcePath)
  try {
    $partNumber = 1
    while ($source.Position -lt $source.Length) {
      $partPath = Join-Path $DestinationDirectory ("part-{0:D4}" -f $partNumber)
      $destination = [IO.File]::Open($partPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
      try {
        $remaining = [Int64]$partSize
        while ($remaining -gt 0) {
          $readLength = [Math]::Min([Int64]$buffer.Length, $remaining)
          $read = $source.Read($buffer, 0, [int]$readLength)
          if ($read -eq 0) { break }
          $destination.Write($buffer, 0, $read)
          $remaining -= $read
        }
      }
      finally {
        $destination.Dispose()
      }
      $parts.Add($partPath)
      $partNumber += 1
    }
  }
  finally {
    $source.Dispose()
  }
  if ($parts.Count -eq 0) {
    $partPath = Join-Path $DestinationDirectory 'part-0001'
    [IO.File]::WriteAllBytes($partPath, (New-Object byte[] 0))
    $parts.Add($partPath)
  }
  return $parts
}

function Invoke-DiagnosticUpload {
  if (-not (Test-UploadConfiguration)) { return $false }
  $uploadId = Get-UploadId
  $uploadHost = $script:UploadHost.TrimEnd('/')
  $fileName = [IO.Path]::GetFileName($script:UploadFile)
  try {
    $fileDigest = Get-FileDigest $script:UploadFile
  }
  catch {
    Write-Log Error "failed to calculate SHA-256 for $($script:UploadFile)"
    return $false
  }
  $temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("esdiag-lite-upload-{0}" -f [Guid]::NewGuid().ToString('N'))
  [IO.Directory]::CreateDirectory($temporaryDirectory) | Out-Null
  try {
    $parts = New-UploadParts $script:UploadFile $temporaryDirectory
    $partNumber = 1
    foreach ($part in $parts) {
      $partDigest = Get-FileDigest $part
      $partUrl = "$uploadHost/api/uploads/$uploadId/$fileDigest/$partDigest"
      $exists = $false
      try {
        Invoke-WebRequest -UseBasicParsing -Method Head -Uri $partUrl -ErrorAction Stop | Out-Null
        $exists = $true
      }
      catch {
        $exists = $false
      }
      if ($exists) {
        Write-Log Info "skipping uploaded part $partNumber"
      }
      else {
        $query = "part_number=$partNumber&part_digest=$partDigest&file_digest=$fileDigest&filename=$([Uri]::EscapeDataString($fileName))"
        try {
          Invoke-WebRequest -UseBasicParsing -Method Put -Uri "$uploadHost/api/uploads/$uploadId?$query" -InFile $part -ContentType 'application/octet-stream' -ErrorAction Stop | Out-Null
          Write-Log Info "uploaded part $partNumber"
        }
        catch {
          Write-Log Error "failed to upload part $partNumber"
          return $false
        }
      }
      $partNumber += 1
    }
    Invoke-WebRequest -UseBasicParsing -Method Post -Uri "$uploadHost/api/uploads/$uploadId/$fileDigest/_finalize" -ErrorAction Stop | Out-Null
    Write-Log Info "uploaded $($script:UploadFile)"
    return $true
  }
  catch {
    Write-Log Error "failed to finalize upload for $($script:UploadFile)"
    return $false
  }
  finally {
    Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
  }
}

function Invoke-Collection {
  $date = [DateTime]::Now.ToString('yyyyMMdd-HHmmss')
  $script:Directory = "api-diagnostics-$date"
  try {
    [IO.Directory]::CreateDirectory((Join-Path $script:Directory 'commercial')) | Out-Null
  }
  catch {
    Write-Log Error "failed to create directory $($script:Directory)"
    return $false
  }
  Write-Log Info "created directory $($script:Directory)"
  if (-not (Get-ApiVersion)) {
    Write-Log Error 'failed to fetch Elasticsearch root response'
    return $false
  }
  if (-not (Read-ClusterVersion)) { return $false }
  if (-not (Invoke-LiteApis)) {
    Write-Log Error 'one or more Elasticsearch API requests failed'
    return $false
  }
  Save-Manifest
  if (-not (Complete-Archive)) { return $false }
  if ($script:UploadRequested) { return Invoke-DiagnosticUpload }
  return $true
}

function Invoke-Watch {
  $jobs = @()
  Write-Log Info "collecting $($script:CollectionCount) diagnostics, $($script:WaitSeconds) seconds apart, from $($env:ELASTIC_ES_URL)"
  for ($number = 1; $number -le $script:CollectionCount; $number += 1) {
    Write-Log Info "collecting diagnostic $number of $($script:CollectionCount)"
    $arguments = @('collect', "--archive=$($script:ArchiveFormat)")
    if ($script:UploadRequested) { $arguments += "--upload=$($script:UploadId)" }
    $jobs += Start-Job -ScriptBlock {
      param($ScriptPath, $ScriptArguments)
      & $ScriptPath @ScriptArguments
      exit $LASTEXITCODE
    } -ArgumentList $PSCommandPath, (,$arguments)
    if ($number -lt $script:CollectionCount) { Start-Sleep -Seconds $script:WaitSeconds }
  }
  $success = $true
  foreach ($job in $jobs) {
    Wait-Job -Job $job | Out-Null
    Receive-Job -Job $job
    if ($job.State -ne 'Completed') { $success = $false }
    Remove-Job -Job $job -Force
  }
  return $success
}

function Invoke-Main {
  if ($PSVersionTable.PSVersion.Major -lt 5) {
    Write-Log Error 'esdiag-lite.ps1 requires Windows PowerShell 5.1 or newer'
    return $false
  }
  if (-not (Parse-Arguments)) { return $false }
  if ($script:CommandName -eq 'help') { Show-Help; return $true }
  if ($script:CommandName -eq 'upload') {
    return (Test-UploadConfiguration) -and (Test-Dependencies) -and (Invoke-DiagnosticUpload)
  }
  if ($script:UploadRequested -and $script:ArchiveFormat -ne 'zip') {
    Write-Log Error 'uploads require --archive=zip'
    return $false
  }
  if (-not (Test-Configuration) -or -not (Test-Dependencies)) { return $false }
  if ($script:CommandName -eq 'watch') { return Invoke-Watch }
  Write-Log Info "collecting diagnostic from $($env:ELASTIC_ES_URL)"
  return Invoke-Collection
}

if ($MyInvocation.InvocationName -ne '.') {
  if (-not (Invoke-Main)) { exit 1 }
}
