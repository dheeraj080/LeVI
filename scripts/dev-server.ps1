#!/usr/bin/env pwsh
# A tiny local target for manual end-to-end checks of `levi request`.
# Responds to any request with a JSON echo of the method, path, auth, and body.
#
#   pwsh scripts/dev-server.ps1 -Port 8787
#
param([int]$Port = 8787)

$listener = [System.Net.HttpListener]::new()
$listener.Prefixes.Add("http://127.0.0.1:$Port/")
$listener.Start()
Write-Host "dev-server listening on http://127.0.0.1:$Port/"

while ($listener.IsListening) {
    $context = $listener.GetContext()
    $request = $context.Request

    $reader = New-Object System.IO.StreamReader($request.InputStream)
    $body = $reader.ReadToEnd()

    $payload = [ordered]@{
        method        = $request.HttpMethod
        path          = $request.Url.AbsolutePath
        query         = $request.Url.Query
        authorization = $request.Headers['Authorization']
        contentType   = $request.ContentType
        body          = $body
    } | ConvertTo-Json -Compress

    $bytes = [System.Text.Encoding]::UTF8.GetBytes($payload)
    $context.Response.StatusCode = 200
    $context.Response.ContentType = 'application/json'
    $context.Response.OutputStream.Write($bytes, 0, $bytes.Length)
    $context.Response.Close()
}
