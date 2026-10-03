$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$collector = Join-Path $repoRoot 'bin/esdiag-lite.ps1'
$tokens = $null
$parseErrors = $null
[System.Management.Automation.Language.Parser]::ParseFile($collector, [ref]$tokens, [ref]$parseErrors) | Out-Null
if ($parseErrors.Count -ne 0) {
  throw "PowerShell parser reported errors: $($parseErrors -join '; ')"
}

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) { throw $Message }
}

function Assert-Equal {
  param($Expected, $Actual, [string]$Message)
  if ($Expected -ne $Actual) { throw "$Message; expected '$Expected', got '$Actual'" }
}

$temporaryDirectory = Join-Path ([IO.Path]::GetTempPath()) ("esdiag-lite-ps-test-{0}" -f [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($temporaryDirectory) | Out-Null
$originalLocation = Get-Location
$originalCurrentDirectory = [Environment]::CurrentDirectory

try {
  Push-Location $temporaryDirectory
  [Environment]::CurrentDirectory = $temporaryDirectory
  $global:Requests = New-Object System.Collections.Generic.List[object]
  $global:UploadPartExists = $false

  . $collector

  function global:Invoke-WebRequest {
    [CmdletBinding()]
    param(
      [switch]$UseBasicParsing,
      [string]$Uri,
      [hashtable]$Headers,
      [string]$OutFile,
      [string]$Method = 'Get',
      [string]$InFile,
      [string]$ContentType
    )

    $global:Requests.Add([PSCustomObject]@{ Uri = $Uri; Headers = $Headers; Method = $Method; InFile = $InFile })
    if ($Method -eq 'Head' -and -not $global:UploadPartExists) {
      throw 'part does not exist'
    }
    if ($OutFile) {
      [IO.Directory]::CreateDirectory((Split-Path -Parent $OutFile)) | Out-Null
      $content = if ($Uri -match '/$') {
        '{"version":{"number":"7.10.0"}}'
      }
      else {
        '{}'
      }
      [IO.File]::WriteAllText($OutFile, $content)
    }
    return [PSCustomObject]@{ StatusCode = 200 }
  }

  function global:Compress-Archive {
    [CmdletBinding()]
    param([string[]]$Path, [string]$DestinationPath, [switch]$Force)
    [IO.File]::WriteAllText($DestinationPath, 'mock archive')
  }

  $env:ELASTIC_ES_URL = 'https://cluster.example'
  $env:ELASTIC_ES_API_KEY = 'api-key'
  $env:ELASTIC_ES_USERNAME = 'username'
  $env:ELASTIC_ES_PASSWORD = 'password'
  Assert-True (Test-Configuration) 'API-key configuration should be accepted'
  Assert-Equal 'api_key' $script:AuthMode 'API key should take precedence over basic authentication'
  Assert-Equal 'ApiKey api-key' (Get-RequestHeaders).Authorization 'API key header should be selected'

  $script:Directory = 'generated-functions'
  $script:ClusterVersion = '7.10.0'
  $script:EsMajor = 7
  $script:EsMinor = 10
  $script:EsPatch = 0
  Assert-True (Get-ApiDataStream) '7.10 data stream request should succeed'
  Assert-Equal 'https://cluster.example/_data_stream' $global:Requests[$global:Requests.Count - 1].Uri '7.10 should use the pre-7.11 data stream API'

  $script:EsMinor = 11
  Assert-True (Get-ApiDataStream) '7.11 data stream request should succeed'
  Assert-Equal 'https://cluster.example/_data_stream?expand_wildcards=all' $global:Requests[$global:Requests.Count - 1].Uri '7.11 should use the expanded data stream API'

  $script:ArchiveFormat = 'zip'
  $script:UploadRequested = $false
  Assert-True (Invoke-Collection) 'collection should complete with mocked requests'
  $archive = Get-ChildItem -Filter 'api-diagnostics-*.zip' | Select-Object -First 1
  Assert-True ($null -ne $archive) 'collection should create a ZIP archive'
  Assert-True ($global:Requests.Uri -contains 'https://cluster.example/_tasks?human&detailed=true') 'collection should request generated lite APIs'

  $script:UploadFile = $archive.FullName
  $script:UploadId = 'upload-id'
  $script:UploadHost = 'https://upload.elastic.co'
  $script:UploadRequested = $true
  Assert-True (Invoke-DiagnosticUpload) 'upload should complete with mocked requests'
  Assert-True ($global:Requests.Method -contains 'Head') 'upload should check whether a part already exists'
  Assert-True ($global:Requests.Method -contains 'Put') 'upload should send a missing part'
  Assert-True ($global:Requests.Method -contains 'Post') 'upload should finalize the file'
  $putRequest = $global:Requests | Where-Object Method -EQ 'Put' | Select-Object -Last 1
  Assert-True ($putRequest.Uri.StartsWith('https://upload.elastic.co/api/uploads/upload-id?part_number=1&')) 'upload URL should retain the upload id and query separator'

  $requestCount = $global:Requests.Count
  $global:UploadPartExists = $true
  Assert-True (Invoke-DiagnosticUpload) 'upload should resume when all parts already exist'
  $resumeMethods = $global:Requests[$requestCount..($global:Requests.Count - 1)].Method
  Assert-True (-not ($resumeMethods -contains 'Put')) 'upload should skip existing parts'

  # Use a real child PowerShell process and jobs: mocks cannot validate job
  # argument serialization, working directories, or child exit statuses.
  $env:ELASTIC_ES_URL = 'http://127.0.0.1:1'
  $env:COLLECTION_COUNT = '1'
  $watchLog = Join-Path $temporaryDirectory 'watch.log'
  $shell = (Get-Process -Id $PID).Path
  $watchProcess = Start-Process -FilePath $shell -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', ('"{0}"' -f $collector), 'watch', '--archive=none') -WorkingDirectory $temporaryDirectory -RedirectStandardOutput $watchLog -RedirectStandardError (Join-Path $temporaryDirectory 'watch-error.log') -WindowStyle Hidden -Wait -PassThru
  Assert-Equal 1 $watchProcess.ExitCode 'watch should fail when a child collection fails'
  $watchOutput = Get-Content -LiteralPath $watchLog -Raw
  Assert-True ($watchOutput -match 'saving / to') 'watch should pass collect arguments to its child'
  Assert-True (@(Get-ChildItem -LiteralPath $temporaryDirectory -Directory -Filter 'api-diagnostics-*').Count -eq 1) 'watch should create its diagnostic in the invoking directory'

  Write-Host 'esdiag-lite PowerShell tests passed'
}
finally {
  [Environment]::CurrentDirectory = $originalCurrentDirectory
  Pop-Location
  Remove-Item -LiteralPath $temporaryDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
