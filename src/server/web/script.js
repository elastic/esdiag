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
function updateStatus({ status }) {
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

// HTMX upload event handlers
document.addEventListener("htmx:beforeRequest", function (evt) {
  if (evt.detail.target.id === "upload-result") {
    updateStatus({ status: "uploading" });

    const progressContainer = document.getElementById(
      "upload-progress-container",
    );
    const progressText = document.getElementById("upload-progress-text");
    const progressBar = document.getElementById("upload-progress-bar");

    // Show progress elements
    progressContainer.classList.add("active");
    progressText.classList.add("active");

    // Since HTMX doesn't support progress tracking, show indeterminate progress
    progressBar.style.width = "100%";
    progressText.textContent = "Uploading...";
  }
});

document.addEventListener("htmx:afterRequest", function (evt) {
  if (evt.detail.target.id === "upload-result") {
    const progressContainer = document.getElementById(
      "upload-progress-container",
    );
    const progressText = document.getElementById("upload-progress-text");
    const progressBar = document.getElementById("upload-progress-bar");

    // Hide progress elements after upload
    setTimeout(() => {
      progressContainer.classList.remove("active");
      progressText.classList.remove("active");
      progressBar.style.width = "0%";
      progressText.textContent = "0%";
      updateFileInfo(null);
    }, 1000);
  }
});

document.addEventListener("htmx:responseError", function (evt) {
  if (evt.detail.target.id === "upload-result") {
    const progressContainer = document.getElementById(
      "upload-progress-container",
    );
    const progressText = document.getElementById("upload-progress-text");

    // Hide progress elements on error
    progressContainer.classList.remove("active");
    progressText.classList.remove("active");
    updateFileInfo(null);
    document.getElementById("drop-area").style.pointerEvents = "";
  }
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

// HTMX upload service event handlers
document.addEventListener("htmx:beforeRequest", function (evt) {
  if (evt.detail.target.id === "upload-service-result") {
    const serviceButton = document.getElementById("upload-service-button");
    serviceButton.disabled = true;
    serviceButton.value = "Submitting...";
  }
});

document.addEventListener("htmx:afterRequest", function (evt) {
  if (evt.detail.target.id === "upload-service-result") {
    const serviceButton = document.getElementById("upload-service-button");
    serviceButton.disabled = false;
    serviceButton.value = "Submit";

    // Clear form on successful request
    if (evt.detail.xhr.status >= 200 && evt.detail.xhr.status < 300) {
      document.getElementById("upload-service-form").reset();
      clearCurlForm();
    }
  }
});

document.addEventListener("htmx:responseError", function (evt) {
  if (evt.detail.target.id === "upload-service-result") {
    const serviceButton = document.getElementById("upload-service-button");
    serviceButton.disabled = false;
    serviceButton.value = "Submit";
  }
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
