function cleanServiceLinkValue(value, trimTrailingPeriod = false) {
    if (!value) {
        return null;
    }

    let cleaned = value.trim();

    // Case comments and copied curl commands often escape shell quotes as \"...\".
    cleaned = cleaned.replace(/\\"/g, '"').replace(/\\'/g, "'");

    while (
        (cleaned.startsWith('"') && cleaned.endsWith('"')) ||
        (cleaned.startsWith("'") && cleaned.endsWith("'"))
    ) {
        cleaned = cleaned.slice(1, -1).trim();
    }

    cleaned = cleaned.replace(/^['"]+|['"]+$/g, "");

    if (trimTrailingPeriod) {
        cleaned = cleaned.replace(/\.$/, "");
    }

    return cleaned || null;
}

function parseToken(cmd) {
    const token = cmd.match(/Authorization(?: Token)?:\s*(?:"([^"]+)"|'([^']+)'|(\S+))/);
    return token ? cleanServiceLinkValue(token[1] || token[2] || token[3]) : null;
}

function parseFilename(cmd) {
    const filename =
        cmd.match(/File name:\s*(?:"([^"]+)"|'([^']+)'|(\S+))/) ||
        cmd.match(/-o\s+(?:"([^"]+)"|'([^']+)'|(\S+))/) ||
        cmd.match(/--output\s+(?:"([^"]+)"|'([^']+)'|(\S+))/);
    return filename ? cleanServiceLinkValue(filename[1] || filename[2] || filename[3]) : null;
}

function parseUrl(cmd) {
    const match = cmd.match(/https?:\/\/[^\s"'\\]+/);
    return match ? cleanServiceLinkValue(match[0], true) : null;
}
