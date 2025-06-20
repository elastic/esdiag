pub static HTML: &'static str = r#"<!DOCTYPE html>
<html>
    <head>
        <title>Elasticsearch Diagnostics Upload</title>
        <style>
            body {
                font-family: Arial, sans-serif;
                max-width: 600px;
                margin: 0 auto;
                padding: 20px;
            }
            h1 {
                color: #005571;
            }
            .upload-form {
                border: 1px solid #ddd;
                padding: 20px;
                border-radius: 5px;
                background-color: #f9f9f9;
            }
            .button {
                background-color: #005571;
                color: white;
                padding: 10px 15px;
                border: none;
                border-radius: 4px;
                cursor: pointer;
                margin-top: 10px;
            }
            .button:hover {
                background-color: #00435a;
            }
            #status-container {
                margin-top: 20px;
                padding: 15px;
                border-radius: 5px;
            }
            .ready {
                background-color: #dddddd;
                border: 1px solid #aaaaaa;
            }
            .success {
                background-color: #e6f4ea;
                border: 1px solid #34a853;
            }
            .error {
                background-color: #fce8e6;
                border: 1px solid #ea4335;
            }
            .processing {
                background-color: #e8f0fe;
                border: 1px solid #4285f4;
            }
            .spinner {
                display: inline-block;
                width: 20px;
                height: 20px;
                border: 3px solid rgba(0, 85, 113, 0.3);
                border-radius: 50%;
                border-top-color: #005571;
                animation: spin 1s ease-in-out infinite;
                margin-right: 10px;
                vertical-align: middle;
            }
            .hidden {
                display: none;
            }
            /* Specific styling for spinner in processing state */
            .processing .spinner {
                border: 3px solid rgba(66, 133, 244, 0.3);
                border-top-color: #4285f4;
            }
            @keyframes spin {
                to { transform: rotate(360deg); }
            }
        </style>
    </head>

    <body>
        <h1>Elastic Stack Diagnostics Upload</h1>
        <div class="upload-form">
            <form id="upload-form" action="/upload" method="post" enctype="multipart/form-data">
                <p>Select a diagnostic bundle (.zip file):</p>
                <input type="file" name="file" accept=".zip" required>
                <br>
                <input type="submit" value="Upload" class="button" id="upload-button">
            </form>
        </div>
        <div id="status-container" class="hidden"></div>

        <script>
            function setStatus({status, report, error, message}) {
                const statusContainer = document.getElementById('status-container');
                console.log("setStatus:",{status, report, error, message});

                switch (status) {
                    case 'uploading':
                        document.getElementById('upload-button').disabled = true;
                        statusContainer.className = '';
                        statusContainer.classList.add('processing');
                        statusContainer.innerHTML = `<p>
                            <div class="spinner"></div>
                            <span>Uploading diagnostic bundle...</span>
                        </p>`;
                        break;
                    case 'processing':
                        document.getElementById('upload-button').disabled = true;
                        statusContainer.className = '';
                        statusContainer.classList.add('processing');
                        statusContainer.innerHTML = `<p>
                            <div class="spinner"></div>
                            <span>${message || "Processing diagnostic..."}</span>
                        </p>`;
                        setTimeout(pollStatus, 1000);
                        break;
                    case 'complete':
                        document.getElementById('upload-button').disabled = false;
                        statusContainer.className = '';
                        statusContainer.classList.add('success');
                        let message_complete = `<p>✅ Diagnostic processing complete!</p>`;
                        if (report) {
                            message_complete += `<p><b>Diagnostic ID:</b> ${report.id}</p>`;
                            message_complete += `<p><b>Product:</b> ${report.product}</p>`;
                            message_complete += `<p><b>Ingested:</b> ${report.docs.created} documents</p>`;
                        }
                        statusContainer.innerHTML = message_complete;
                        break;
                    case 'error':
                        document.getElementById('upload-button').disabled = false;
                        statusContainer.className = '';
                        statusContainer.classList.add('error');
                        statusContainer.innerHTML = `<p>🛑 <b>Error:</b> ${error || 'Processing failed'}</p>`;
                        break;
                    case 'ready':
                        document.getElementById('upload-button').disabled = false;
                        statusContainer.className = '';
                        statusContainer.classList.add('ready');
                        statusContainer.innerHTML = `<p>▶️ Ready for upload</p>`;
                }
            }

            function pollStatus() {
                fetch(`/status`)
                    .then(response => response.json())
                    .then(data => setStatus(data))
                    .catch(error => {
                        console.error('Polling error:', error);
                    });
            }

            document.getElementById('upload-form').addEventListener('submit', function(e) {
                e.preventDefault();
                const statusContainer = document.getElementById('status-container');
                const formData = new FormData(this);
                setStatus({status: 'uploading'});
                fetch('/upload', {
                    method: 'POST',
                    body: formData
                }).then(response => {
                    if (!response.ok) {
                        response.json().then(error => setStatus(error));
                    } else {
                        response.json().then(data => setStatus(data));
                    }
                }).catch(error => {
                    setStatus({status:'error', error});
                });
            });

            setTimeout(pollStatus, 500);
        </script>
    </body>
</html>
"#;
