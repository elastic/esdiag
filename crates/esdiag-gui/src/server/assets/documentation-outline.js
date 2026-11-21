class DocumentationOutline extends HTMLElement {
    constructor() {
        super();
        this._initialized = false;
        this._retryCount = 0;
        this._flatNodes = [];
        this._activeIndex = 0;
        this._lastScrollY = 0;
        this._pointerDown = false;
        this._pendingMouseId = null;
        this._rafPending = false;
        this._suppressViewportUntil = 0;
        this._linkById = new Map();
        this._topScopeById = new Map();
        this._topScopeElements = new Map();
        this._headingById = new Map();
        this._bodyObserver = null;
        this._observedBody = null;
        this._refreshPending = false;

        this._onViewportChanged = this._onViewportChanged.bind(this);
        this._onMouseDown = this._onMouseDown.bind(this);
        this._onMouseUp = this._onMouseUp.bind(this);
        this._onClick = this._onClick.bind(this);
        this._onBodyMutated = this._onBodyMutated.bind(this);
    }

    connectedCallback() {
        this._observeDocumentBody();
        queueMicrotask(() => this._initialize());
    }

    _initialize() {
        this._observeDocumentBody();

        const outline = this._resolveOutlineData();
        if (!Array.isArray(outline) || outline.length === 0) {
            if (this._retryCount < 12) {
                this._retryCount += 1;
                requestAnimationFrame(() => this._initialize());
            }
            return;
        }
        this._retryCount = 0;

        this._flatNodes = this._flattenOutline(outline);
        if (this._flatNodes.length === 0) {
            return;
        }

        this._unbindEvents();
        this._render(outline);
        this._cacheReferences();
        this._bindEvents();
        this._activeIndex = this._resolveActiveIndex();
        this._applyActiveState(this._activeIndex);
        this._initialized = true;
    }

    disconnectedCallback() {
        this._unbindEvents();
        this._disconnectBodyObserver();
        this._initialized = false;
        this._retryCount = 0;
        this._refreshPending = false;
    }

    _readOutlineData() {
        const dataScript = this.querySelector(
            'script[type="application/json"][data-outline]',
        );
        if (!dataScript) return null;

        try {
            return JSON.parse(dataScript.textContent || "[]");
        } catch (error) {
            console.error("Invalid documentation-outline JSON:", error);
            return null;
        }
    }

    _resolveOutlineData() {
        const explicitOutline = this._readOutlineData();
        if (Array.isArray(explicitOutline) && explicitOutline.length > 0) {
            return explicitOutline;
        }
        return this._buildOutlineFromDocumentBody();
    }

    _buildOutlineFromDocumentBody() {
        const body = this._resolveDocumentBody();
        if (!body) return [];

        const headings = Array.from(
            body.querySelectorAll(
                "h1[id], h2[id], h3[id], page-title[id], section-title[id]",
            ),
        );
        if (headings.length === 0) return [];

        const outline = [];
        let firstPageTitleSkipped = false;
        let currentH2 = null;

        headings.forEach((heading) => {
            const normalized = this._normalizeHeading(heading);
            if (!normalized) return;

            if (normalized.level === 1 && !firstPageTitleSkipped) {
                firstPageTitleSkipped = true;
                currentH2 = null;
                return;
            }

            const node = {
                id: normalized.id,
                label: normalized.label,
                level: normalized.level,
            };

            if (normalized.level === 1) {
                outline.push(node);
                currentH2 = null;
                return;
            }

            if (normalized.level === 2) {
                outline.push(node);
                currentH2 = node;
                return;
            }

            if (normalized.level === 3 && currentH2) {
                if (!Array.isArray(currentH2.children)) {
                    currentH2.children = [];
                }
                currentH2.children.push(node);
                return;
            }

            outline.push(node);
        });

        return outline;
    }

    _normalizeHeading(heading) {
        const tagName = heading.tagName.toLowerCase();
        const levelMap = {
            h1: 1,
            h2: 2,
            h3: 3,
            "page-title": 1,
            "section-title": 2,
        };
        const level = levelMap[tagName];
        if (!Number.isFinite(level) || level < 1 || level > 3) return null;

        const id = heading.id.trim();
        if (!id) return null;

        const label = heading.textContent?.trim().replace(/\s+/g, " ") || id;
        return { id, label, level };
    }

    _resolveDocumentBody() {
        const viewer = this.closest("documentation-viewer");
        return this.previousElementSibling?.tagName?.toLowerCase() ===
            "documentation-body"
            ? this.previousElementSibling
            : viewer?.querySelector("documentation-body");
    }

    _flattenOutline(nodes, depth = 1, topScopeId = null, acc = []) {
        if (depth > 3 || !Array.isArray(nodes)) return acc;

        nodes.forEach((node) => {
            if (
                !node ||
                typeof node.id !== "string" ||
                typeof node.label !== "string"
            ) {
                return;
            }

            const nodeLevel = Number.isFinite(node.level) ? node.level : depth;
            const nodeTopScopeId =
                nodeLevel === 2 ? node.id : topScopeId || null;
            acc.push({
                id: node.id,
                label: node.label,
                depth,
                level: nodeLevel,
                topScopeId: nodeTopScopeId,
            });

            this._flattenOutline(node.children, depth + 1, nodeTopScopeId, acc);
        });

        return acc;
    }

    _render(outline) {
        this.innerHTML = this._renderList(outline, 1, null);
    }

    _renderList(nodes, depth, topScopeId) {
        const items = nodes
            .filter(
                (node) =>
                    node &&
                    typeof node.id === "string" &&
                    typeof node.label === "string",
            )
            .map((node) => {
                const escapedId = this._escapeHtml(node.id);
                const escapedLabel = this._escapeHtml(node.label);
                const nodeLevel = Number.isFinite(node.level)
                    ? node.level
                    : depth;
                const scopeId = topScopeId || (nodeLevel === 2 ? node.id : "");
                const escapedScopeId = this._escapeHtml(scopeId);
                const childHtml =
                    depth < 3 &&
                    Array.isArray(node.children) &&
                    node.children.length > 0
                        ? this._renderList(
                              node.children,
                              depth + 1,
                              topScopeId || node.id,
                          )
                        : "";

                return `<li data-outline-id="${escapedId}" data-outline-level="${nodeLevel}" data-outline-scope="${escapedScopeId}"><a href="#${escapedId}">${escapedLabel}</a>${childHtml}</li>`;
            })
            .join("");

        return `<ul>${items}</ul>`;
    }

    _cacheReferences() {
        this._linkById.clear();
        this._topScopeById.clear();
        this._topScopeElements.clear();
        this._headingById.clear();

        const links = this.querySelectorAll('a[href^="#"]');
        links.forEach((link) => {
            const id = link.getAttribute("href").slice(1);
            this._linkById.set(id, link);
        });

        this._flatNodes.forEach((node) => {
            this._topScopeById.set(node.id, node.topScopeId);
            const heading = document.getElementById(node.id);
            if (heading) this._headingById.set(node.id, heading);
        });

        const topItems = this.querySelectorAll(
            ':scope > ul > li[data-outline-level="2"]',
        );
        topItems.forEach((item) => {
            const scopeId = item.getAttribute("data-outline-id");
            if (scopeId) this._topScopeElements.set(scopeId, item);
        });
    }

    _bindEvents() {
        if (this._eventsBound) return;
        window.addEventListener("scroll", this._onViewportChanged, {
            passive: true,
        });
        window.addEventListener("resize", this._onViewportChanged);
        window.addEventListener("mouseup", this._onMouseUp, true);
        this.addEventListener("mousedown", this._onMouseDown);
        this.addEventListener("click", this._onClick);
        this._eventsBound = true;
    }

    _unbindEvents() {
        if (!this._eventsBound) return;
        window.removeEventListener("scroll", this._onViewportChanged);
        window.removeEventListener("resize", this._onViewportChanged);
        window.removeEventListener("mouseup", this._onMouseUp, true);
        this.removeEventListener("mousedown", this._onMouseDown);
        this.removeEventListener("click", this._onClick);
        this._eventsBound = false;
    }

    _observeDocumentBody() {
        const body = this._resolveDocumentBody();
        if (!body || body === this._observedBody) return;

        this._disconnectBodyObserver();
        this._bodyObserver = new MutationObserver(this._onBodyMutated);
        this._bodyObserver.observe(body, {
            childList: true,
            subtree: true,
            characterData: true,
        });
        this._observedBody = body;
    }

    _disconnectBodyObserver() {
        if (this._bodyObserver) {
            this._bodyObserver.disconnect();
            this._bodyObserver = null;
        }
        this._observedBody = null;
    }

    _onBodyMutated() {
        if (this._refreshPending) return;
        this._refreshPending = true;
        requestAnimationFrame(() => {
            requestAnimationFrame(() => {
                this._refreshPending = false;
                this._retryCount = 0;
                this._initialize();
            });
        });
    }

    _clear() {
        this._flatNodes = [];
        this._activeIndex = 0;
        this._linkById.clear();
        this._topScopeById.clear();
        this._topScopeElements.clear();
        this._headingById.clear();
        this._unbindEvents();
        this.innerHTML = "";
        this._initialized = false;
    }

    _onViewportChanged() {
        if (this._pointerDown || this._rafPending) return;
        if (performance.now() < this._suppressViewportUntil) return;
        this._rafPending = true;
        requestAnimationFrame(() => {
            this._rafPending = false;
            const nextIndex = this._resolveActiveIndex();
            this._applyActiveState(nextIndex);
        });
    }

    _onMouseDown(event) {
        const link = this._eventLink(event);
        if (!link || !this.contains(link)) return;
        event.preventDefault();

        const id = link.getAttribute("href").slice(1);
        this._pointerDown = true;
        this._pendingMouseId = id;
        this._smoothScrollToId(id);
    }

    _onMouseUp() {
        const id = this._pendingMouseId;
        if (!id) return;
        this._pendingMouseId = null;
        this._pointerDown = false;
        this._applyActiveState(this._indexById(id));
    }

    _onClick(event) {
        const link = this._eventLink(event);
        if (!link || !this.contains(link)) return;
        event.preventDefault();

        if (event.detail === 0) {
            const id = link.getAttribute("href").slice(1);
            this._smoothScrollToId(id);
            this._applyActiveState(this._indexById(id));
        }
    }

    _resolveActiveIndex() {
        if (this._flatNodes.length === 0) return 0;

        const maxIndex = this._flatNodes.length - 1;
        if (window.scrollY <= 2) {
            this._lastScrollY = window.scrollY;
            return 0;
        }

        if (
            window.innerHeight + window.scrollY >=
            document.documentElement.scrollHeight - 2
        ) {
            this._lastScrollY = window.scrollY;
            return maxIndex;
        }

        const header = document.querySelector("header");
        const headerBottom = header
            ? header.getBoundingClientRect().bottom
            : 52;
        const goingUp = window.scrollY < this._lastScrollY;
        let index = Math.max(0, Math.min(maxIndex, this._activeIndex));

        const headingTop = (idx) => {
            const id = this._flatNodes[idx]?.id;
            const heading = id ? this._headingById.get(id) : null;
            return heading
                ? heading.getBoundingClientRect().top
                : Number.POSITIVE_INFINITY;
        };

        if (goingUp) {
            if (index > 0 && headingTop(index - 1) >= headerBottom) {
                index -= 1;
            }
        } else {
            if (index < maxIndex && headingTop(index + 1) <= headerBottom) {
                index += 1;
            }
        }

        this._lastScrollY = window.scrollY;
        return index;
    }

    _applyActiveState(index) {
        const maxIndex = this._flatNodes.length - 1;
        const safeIndex = Math.max(0, Math.min(maxIndex, index));
        this._activeIndex = safeIndex;
        const activeId = this._flatNodes[safeIndex]?.id;
        if (!activeId) return;

        this._linkById.forEach((link, id) => {
            const active = id === activeId;
            link.classList.toggle("active-item", active);
            if (active) {
                link.setAttribute("aria-current", "location");
            } else {
                link.removeAttribute("aria-current");
            }
        });

        const activeScope = this._topScopeById.get(activeId);
        this._topScopeElements.forEach((element, scopeId) => {
            element.classList.toggle("active-scope", scopeId === activeScope);
        });
    }

    _smoothScrollToId(id) {
        const heading = this._headingById.get(id);
        if (!heading) return;

        const header = document.querySelector("header");
        const headerHeight = header
            ? header.getBoundingClientRect().height
            : 52;
        const unclampedTarget = Math.max(
            0,
            heading.getBoundingClientRect().top +
                window.scrollY -
                headerHeight -
                8,
        );
        const maxScrollTop = Math.max(
            0,
            document.documentElement.scrollHeight - window.innerHeight,
        );
        const targetTop = Math.min(unclampedTarget, maxScrollTop);
        const prefersReducedMotion = window.matchMedia(
            "(prefers-reduced-motion: reduce)",
        ).matches;

        if (prefersReducedMotion) {
            this._suppressViewportUntil = performance.now() + 80;
            window.scrollTo(0, targetTop);
        } else {
            const startTop = window.scrollY;
            const delta = targetTop - startTop;
            const durationMs = 140;
            if (Math.abs(delta) < 1) {
                history.replaceState(null, "", `#${id}`);
                return;
            }

            this._suppressViewportUntil = performance.now() + durationMs + 40;
            const startTime = performance.now();
            const easeOutCubic = (value) => 1 - Math.pow(1 - value, 3);

            const tick = (now) => {
                const progress = Math.min((now - startTime) / durationMs, 1);
                const eased = easeOutCubic(progress);
                window.scrollTo(0, startTop + delta * eased);
                if (progress < 1) requestAnimationFrame(tick);
            };

            requestAnimationFrame(tick);
        }

        history.replaceState(null, "", `#${id}`);
    }

    _indexById(id) {
        const idx = this._flatNodes.findIndex((node) => node.id === id);
        return idx === -1 ? 0 : idx;
    }

    _eventLink(event) {
        const target =
            event.target instanceof Element
                ? event.target
                : event.target?.parentElement;
        return target?.closest('a[href^="#"]') ?? null;
    }

    _escapeHtml(value) {
        return String(value)
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;");
    }
}

if (!customElements.get("documentation-outline")) {
    customElements.define("documentation-outline", DocumentationOutline);
}
