// User information
function setUserInfo(username) {
  document.getElementById("user-circle").firstChild.textContent = username
    ? username.charAt(0).toUpperCase()
    : "_";
  document.getElementById("username-display").textContent =
    username || "Anonymous";
}

function updateKibanaUrl(kibanaUrl) {
  const kibanaLink = document.querySelector("#kibana a");
  const kibanaMenu = document.querySelector("#kibana .user-menu-item");

  if (kibanaLink && kibanaUrl) {
    kibanaLink.href = `${kibanaUrl}/app/dashboards#/view/2c8cd284-79ef-4787-8b79-0030e0df467b`;
  }

  if (kibanaMenu && kibanaUrl) {
    kibanaMenu.textContent = kibanaUrl;
  }
}

// File upload handling
const dropArea = document.getElementById("drop-area");
const fileInput = document.getElementById("file-input");
const fileInfo = document.getElementById("file-info");
const fileName = document.getElementById("file-name");
const clearFile = document.getElementById("clear-file");
const uploadInstructions = document.getElementById("upload-instructions");

// Prevent default drag behaviors
["dragenter", "dragover", "dragleave", "drop"].forEach((eventName) => {
  dropArea.addEventListener(eventName, preventDefaults, false);
  document.body.addEventListener(eventName, preventDefaults, false);
});

function preventDefaults(e) {
  e.preventDefault();
  e.stopPropagation();
}

// Highlight drop area when item is dragged over it
["dragenter", "dragover"].forEach((eventName) => {
  dropArea.addEventListener(eventName, highlight, false);
});

["dragleave", "drop"].forEach((eventName) => {
  dropArea.addEventListener(eventName, unhighlight, false);
});

function highlight() {
  dropArea.classList.add("drag-over");
}

function unhighlight() {
  dropArea.classList.remove("drag-over");
}

// Handle dropped files
dropArea.addEventListener("drop", handleDrop, false);

function handleDrop(e) {
  const dt = e.dataTransfer;
  const files = dt.files;
  handleFiles(files);
}

function handleFiles(files) {
  if (files.length > 0) {
    fileInput.files = files;
    updateFileInfo(files[0]);
  }
}

function updateFileInfo(file) {
  const uploadButton = document.getElementById("upload-button");
  const fileIcon = document.querySelector(".file-icon");
  const currentStatus = document.getElementById("current-status");
  const isUploading =
    currentStatus && currentStatus.textContent.includes("Uploading");

  // Don't clear the file if we're currently uploading
  if (isUploading && !file) return;

  if (file) {
    fileName.textContent = file.name;
    fileInfo.classList.remove("hidden");
    uploadButton.disabled = false;
    uploadInstructions.classList.add("hidden");
    fileIcon.style.color = "#1339ab"; // Change to button text blue

    // Change the icon to a check mark
    document.querySelector(".file-icon svg").innerHTML = `
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>
            <polyline points="14 2 14 8 20 8"></polyline>
            <path d="M9 15L11 17L15 13"></path>
        `;
  } else {
    fileInput.value = "";
    fileInfo.classList.add("hidden");
    uploadButton.disabled = true;
    uploadInstructions.classList.remove("hidden");
    fileIcon.style.color = "#516381"; // Reset to original color

    // Reset to upload icon
    document.querySelector(".file-icon svg").innerHTML = `
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>
            <polyline points="14 2 14 8 20 8"></polyline>
            <line x1="12" y1="18" x2="12" y2="12"></line>
            <line x1="9" y1="15" x2="15" y2="15"></line>
        `;
  }
}

// Open file picker when clicking drop area
dropArea.addEventListener("click", () => {
  fileInput.click();
});

// Handle selected file when using file picker
fileInput.addEventListener("change", function () {
  if (this.files && this.files[0]) {
    updateFileInfo(this.files[0]);
  }
});

// Clear selected file when clicking X button
clearFile.addEventListener("click", function (e) {
  e.stopPropagation();
  updateFileInfo(null);
});

// Initialize button state when page loads
document.addEventListener("DOMContentLoaded", function () {
  const uploadButton = document.getElementById("upload-button");
  uploadButton.disabled = fileInput.files.length === 0;
});

// Simplified status update for upload handling only
function updateStatus({ status, error, user, exporter, kibana, queue = {} }) {
  if (user) {
    setUserInfo(user);
  }

  if (kibana) {
    updateKibanaUrl(kibana);
  }

  const dropArea = document.getElementById("drop-area");

  // Reset upload-related UI elements
  if (status !== "uploading") {
    dropArea.style.borderColor = "";
    dropArea.style.pointerEvents = "";
  }
}

// HTMX event handlers for status updates
document.addEventListener("htmx:afterRequest", function (evt) {
  if (evt.detail.pathInfo.requestPath === "/status") {
    const response = evt.detail.xhr.response;

    // For non-HTMX requests that return JSON, we need to handle them
    if (
      evt.detail.xhr
        .getResponseHeader("content-type")
        ?.includes("application/json")
    ) {
      const data = JSON.parse(response);

      // Update kibana URL if available
      if (data.kibana) {
        updateKibanaUrl(data.kibana);
      }

      setUserInfo(data.user);
    }
  }
});

// HTMX error handler for status requests
document.addEventListener("htmx:responseError", function (evt) {
  if (evt.detail.pathInfo.requestPath === "/status") {
    console.error("Status polling error:", evt.detail.xhr.status);
    // On error, continue polling but with longer interval
  }
});

document.addEventListener("htmx:timeout", function (evt) {
  if (evt.detail.pathInfo.requestPath === "/status") {
    console.error("Status polling timeout");
    // On timeout, continue polling but with longer interval
  }
});

// Handle HTMX trigger for user and Kibana updates
document.addEventListener("updateUserAndKibana", function (evt) {
  const data = evt.detail;
  if (data.user) {
    setUserInfo(data.user);
  }
  if (data.kibana) {
    updateKibanaUrl(data.kibana);
  }
});

document.getElementById("upload-form").addEventListener("submit", function (e) {
  e.preventDefault();
  const formData = new FormData(this);
  // Keep the file visible while uploading
  const currentFile = fileInput.files[0];
  updateStatus({ status: "uploading" });

  // Use XMLHttpRequest to track upload progress
  const xhr = new XMLHttpRequest();
  const progressBar = document.getElementById("upload-progress-bar");
  const progressContainer = document.getElementById(
    "upload-progress-container",
  );
  const progressText = document.getElementById("upload-progress-text");

  // Show progress elements
  progressContainer.classList.add("active");
  progressText.classList.add("active");

  // Reset progress bar
  progressBar.style.width = "0%";
  progressText.textContent = "0%";

  xhr.upload.addEventListener("progress", function (event) {
    if (event.lengthComputable) {
      const percentComplete = (event.loaded / event.total) * 100;
      const percentFormatted = Math.round(percentComplete) + "%";

      progressBar.style.width = percentFormatted;
      progressText.textContent = percentFormatted;
    }
  });

  xhr.onload = function () {
    if (xhr.status >= 200 && xhr.status < 300) {
      const body = JSON.parse(xhr.responseText);
      // Make sure to pass queue information from the response
      updateStatus({
        ...body,
        queue: body.queue || {},
      });
      // Reset progress bar after a delay to show completion
      setTimeout(() => {
        progressBar.style.width = "0%";
        progressContainer.classList.remove("active");
        progressText.classList.remove("active");
        // Only clear file info after upload is complete and progress bar is hidden
        updateFileInfo(null);
        // Re-enable drop area
        document.getElementById("drop-area").style.pointerEvents = "";
      }, 1000);
    } else {
      updateStatus({
        status: "error",
        error: "Upload failed with status: " + xhr.status,
      });
      // Hide progress elements
      progressContainer.classList.remove("active");
      progressText.classList.remove("active");
      // Clear file info after error
      updateFileInfo(null);
      // Re-enable drop area
      document.getElementById("drop-area").style.pointerEvents = "";
    }
  };

  xhr.onerror = function () {
    updateStatus({
      status: "error",
      error: "Upload failed",
    });
    // Hide progress elements
    progressContainer.classList.remove("active");
    progressText.classList.remove("active");
    // Clear file info after error
    updateFileInfo(null);
    // Re-enable drop area
    document.getElementById("drop-area").style.pointerEvents = "";
  };

  xhr.open("POST", "/upload", true);
  xhr.send(formData);
});

// Tab switching functionality
const fileTab = document.getElementById("file-tab");
const serviceTab = document.getElementById("service-tab");
const fileContent = document.getElementById("file-content");
const serviceContent = document.getElementById("service-content");

fileTab.addEventListener("click", () => {
  fileTab.classList.add("active");
  serviceTab.classList.remove("active");
  fileContent.classList.add("active");
  serviceContent.classList.remove("active");
});

serviceTab.addEventListener("click", () => {
  serviceTab.classList.add("active");
  fileTab.classList.remove("active");
  serviceContent.classList.add("active");
  fileContent.classList.remove("active");
});

// Upload service form handler
document
  .getElementById("upload-service-form")
  .addEventListener("submit", function (e) {
    e.preventDefault();

    const formData = new FormData(this);
    const serviceButton = document.getElementById("upload-service-button");

    // Convert FormData to JSON
    const data = {
      token: formData.get("token"),
      url: formData.get("url"),
      metadata: {
        filename: formData.get("filename"),
      },
    };

    // Disable button during request
    serviceButton.disabled = true;
    serviceButton.value = "Submitting...";

    fetch("/upload_service", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(data),
    })
      .then((response) => response.json())
      .then((responseData) => {
        // Immediately show retrieving status if we got a job ID
        if (responseData.job_id) {
          // Create a retrieving status entry
          const historyContainer = document.getElementById("history-container");
          const retrievingItem = document.createElement("div");
          retrievingItem.className = "status-box history-item processing";
          retrievingItem.innerHTML = `
                        <div class="spinner"></div>
                        <span><b>Retrieving:</b> ${formData.get("filename")}</span>
                    `;

          // Insert at the beginning
          historyContainer.insertBefore(
            retrievingItem,
            historyContainer.firstChild,
          );
        }

        updateStatus({
          ...responseData,
          queue: responseData.queue || {},
        });
        // Clear form after successful submission
        document.getElementById("upload-service-form").reset();
      })
      .catch((error) => {
        console.error("Upload service error:", error);
        updateStatus({
          status: "error",
          error: "Upload service request failed",
        });
      })
      .finally(() => {
        // Re-enable button
        serviceButton.disabled = false;
        serviceButton.value = "Submit";
      });
  });

// Curl command parsing
const textareaCurl = document.getElementById("curl-command");
const linkClearCurl = document.getElementById("clear-curl");
const inputToken = document.getElementById("service-token");
const inputUrl = document.getElementById("service-url");
const inputFilename = document.getElementById("service-filename");

function parseCurlCommand(curlCommand) {
  // Extract token from Authorization header
  const tokenMatch =
    curlCommand.match(/-H\s+['"]Authorization:\s*([^'"]+)['"]/) ||
    curlCommand.match(/--header\s+['"]Authorization:\s*([^'"]+)['"]/);

  // Extract filename from -o option
  const filenameMatch =
    curlCommand.match(/-o\s+['"]([^'"]+)['"]/) ||
    curlCommand.match(/-o\s+(\S+)/) ||
    curlCommand.match(/--output\s+['"]([^'"]+)['"]/) ||
    curlCommand.match(/--output\s+(\S+)/);

  // Extract URL (last argument that starts with http)
  const urlMatch = curlCommand.match(/https?:\/\/\S+/);

  const parsed = {
    token: tokenMatch ? tokenMatch[1] : null,
    filename: filenameMatch ? filenameMatch[1] : null,
    url: urlMatch ? urlMatch[0] : null,
  };

  // Check if parsing was successful
  parsed.hasError = !(parsed.token && parsed.filename && parsed.url);

  return parsed;
}

function fillFormFromCurl(parsed) {
  // Clear error state first
  textareaCurl.classList.remove("error");

  if (parsed.hasError) {
    // Show error state if parsing failed
    textareaCurl.classList.add("error");
    return;
  }

  if (parsed.token) {
    inputToken.value = parsed.token;
    inputToken.readOnly = true;
  }
  if (parsed.url) {
    inputUrl.value = parsed.url;
    inputUrl.readOnly = true;
  }
  if (parsed.filename) {
    inputFilename.value = parsed.filename;
    inputFilename.readOnly = true;
  }

  // Show clear link if any field was filled
  if (parsed.token || parsed.url || parsed.filename) {
    linkClearCurl.classList.add("visible");
  }
}

function clearCurlForm() {
  textareaCurl.value = "";
  textareaCurl.classList.remove("error");
  inputToken.value = "";
  inputUrl.value = "";
  inputFilename.value = "";
  inputToken.readOnly = false;
  inputUrl.readOnly = false;
  inputFilename.readOnly = false;
  linkClearCurl.classList.remove("visible");
}

textareaCurl.addEventListener("input", function () {
  const curlCommand = this.value.trim();

  if (curlCommand && curlCommand.startsWith("curl")) {
    const parsed = parseCurlCommand(curlCommand);
    fillFormFromCurl(parsed);
  } else if (!curlCommand) {
    clearCurlForm();
  } else {
    textareaCurl.classList.add("error");
    linkClearCurl.classList.add("visible");
  }
});

linkClearCurl.addEventListener("click", clearCurlForm);
