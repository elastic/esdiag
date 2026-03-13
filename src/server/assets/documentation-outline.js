class DocumentationOutline extends HTMLElement {
  constructor() {
    super();
    this._initialized = false;
    this._initRetries = 0;
    this._maxInitRetries = 120;
    this._initRafId = null;
    this._mutationRafId = null;
    this._observer = null;
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

    this._onViewportChanged = this._onViewportChanged.bind(this);
    this._onMouseDown = this._onMouseDown.bind(this);
    this._onMouseUp = this._onMouseUp.bind(this);
    this._onClick = this._onClick.bind(this);
  }

  connectedCallback() {
    if (this._initialized) return;

    this._startObserving();
    queueMicrotask(() => this._initialize());
  }

  _initialize() {
    this._rebuildOutline();
  }

  disconnectedCallback() {
    this._unbindEvents();
    this._initialized = false;
    this._stopObserving();
    this._clearInitializeRetry();
    this._clearMutationFrame();
  }

  _scheduleInitializeRetry() {
    if (this._initRafId !== null) return;
    if (this._initRetries >= this._maxInitRetries) return;
    this._initRetries += 1;
    this._initRafId = requestAnimationFrame(() => {
      this._initRafId = null;
      this._initialize();
    });
  }

  _clearInitializeRetry() {
    if (this._initRafId !== null) {
      cancelAnimationFrame(this._initRafId);
      this._initRafId = null;
    }
    this._initRetries = 0;
  }

  _startObserving() {
    if (this._observer) return;
    const viewer = this.closest("documentation-viewer");
    const body =
      this.previousElementSibling?.tagName?.toLowerCase() === "documentation-body"
        ? this.previousElementSibling
        : viewer?.querySelector("documentation-body");
    const target = body || viewer || this.parentElement;
    if (!target) return;

    this._observer = new MutationObserver(() => {
      if (this._mutationRafId !== null) return;
      this._mutationRafId = requestAnimationFrame(() => {
        this._mutationRafId = null;
        this._rebuildOutline();
      });
    });
    this._observer.observe(target, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["id"],
    });
  }

  _stopObserving() {
    if (!this._observer) return;
    this._observer.disconnect();
    this._observer = null;
  }

  _clearMutationFrame() {
    if (this._mutationRafId !== null) {
      cancelAnimationFrame(this._mutationRafId);
      this._mutationRafId = null;
    }
  }

  _rebuildOutline() {
    const outline = this._resolveOutlineData();
    if (!Array.isArray(outline) || outline.length === 0) {
      if (this._initialized) {
        this._unbindEvents();
        this.innerHTML = "";
        this._flatNodes = [];
        this._activeIndex = 0;
        this._initialized = false;
      }
      this._scheduleInitializeRetry();
      return;
    }

    const flatNodes = this._flattenOutline(outline);
    if (flatNodes.length === 0) {
      if (this._initialized) {
        this._unbindEvents();
        this.innerHTML = "";
        this._flatNodes = [];
        this._activeIndex = 0;
        this._initialized = false;
      }
      this._scheduleInitializeRetry();
      return;
    }

    if (this._initialized) {
      this._unbindEvents();
    }

    this._flatNodes = flatNodes;
    this._render(outline);
    this._cacheReferences();
    this._bindEvents();
    this._activeIndex = this._resolveActiveIndex();
    this._applyActiveState(this._activeIndex);
    this._initialized = true;
    this._clearInitializeRetry();
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
    const viewer = this.closest("documentation-viewer");
    const body =
      this.previousElementSibling?.tagName?.toLowerCase() === "documentation-body"
        ? this.previousElementSibling
        : viewer?.querySelector("documentation-body");
    if (!body) return [];

    const headings = Array.from(
      body.querySelectorAll(
        "h2[id], h3[id], section-title[id]",
      ),
    );
    if (headings.length === 0) return [];

    const outline = [];
    const stack = [];

    headings.forEach((heading) => {
      const tagName = heading.tagName.toLowerCase();
      const levelMap = {
        h2: 2,
        h3: 3,
        "section-title": 2,
      };
      const level = levelMap[tagName];
      if (!Number.isFinite(level) || level < 1 || level > 3) return;

      const id = heading.id.trim();
      if (!id) return;

      const label = heading.textContent?.trim().replace(/\s+/g, " ") || id;
      const node = { id, label, level };

      while (stack.length > 0 && stack[stack.length - 1].level >= level) {
        stack.pop();
      }

      if (stack.length === 0) {
        outline.push(node);
      } else {
        const parentNode = stack[stack.length - 1].node;
        if (!Array.isArray(parentNode.children)) {
          parentNode.children = [];
        }
        parentNode.children.push(node);
      }

      stack.push({ level, node });
    });

    return outline;
  }

  _flattenOutline(nodes, depth = 1, sectionScopeId = null, acc = []) {
    if (depth > 3 || !Array.isArray(nodes)) return acc;

    nodes.forEach((node) => {
      if (
        !node ||
        typeof node.id !== "string" ||
        typeof node.label !== "string"
      ) {
        return;
      }

      const nodeSectionScopeId = node.level === 2 ? node.id : sectionScopeId;
      acc.push({
        id: node.id,
        label: node.label,
        depth,
        sectionScopeId: nodeSectionScopeId,
      });

      this._flattenOutline(node.children, depth + 1, nodeSectionScopeId, acc);
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
          node && typeof node.id === "string" && typeof node.label === "string",
      )
      .map((node) => {
        const escapedId = this._escapeHtml(node.id);
        const escapedLabel = this._escapeHtml(node.label);
        const scopeId = this._escapeHtml(topScopeId || node.id);
        const childHtml =
          depth < 3 && Array.isArray(node.children) && node.children.length > 0
            ? this._renderList(node.children, depth + 1, topScopeId || node.id)
            : "";

        return `<li data-outline-id="${escapedId}" data-outline-scope="${scopeId}"><a href="#${escapedId}">${escapedLabel}</a>${childHtml}</li>`;
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
      this._topScopeById.set(node.id, node.sectionScopeId || null);
      const heading = document.getElementById(node.id);
      if (heading) this._headingById.set(node.id, heading);
    });

    const items = this.querySelectorAll("li[data-outline-id]");
    items.forEach((item) => {
      const id = item.getAttribute("data-outline-id");
      if (id) this._topScopeElements.set(id, item);
    });
  }

  _bindEvents() {
    window.addEventListener("scroll", this._onViewportChanged, { passive: true });
    window.addEventListener("resize", this._onViewportChanged);
    window.addEventListener("mouseup", this._onMouseUp, true);
    this.addEventListener("mousedown", this._onMouseDown);
    this.addEventListener("click", this._onClick);
  }

  _unbindEvents() {
    window.removeEventListener("scroll", this._onViewportChanged);
    window.removeEventListener("resize", this._onViewportChanged);
    window.removeEventListener("mouseup", this._onMouseUp, true);
    this.removeEventListener("mousedown", this._onMouseDown);
    this.removeEventListener("click", this._onClick);
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
    const target =
      event.target instanceof Element ? event.target : event.target?.parentElement;
    if (!target) return;
    const link = target.closest('a[href^="#"]');
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
    const target =
      event.target instanceof Element ? event.target : event.target?.parentElement;
    if (!target) return;
    const link = target.closest('a[href^="#"]');
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
    const headerBottom = header ? header.getBoundingClientRect().bottom : 52;
    const goingUp = window.scrollY < this._lastScrollY;
    let index = Math.max(0, Math.min(maxIndex, this._activeIndex));

    const headingTop = (idx) => {
      const id = this._flatNodes[idx]?.id;
      const heading = id ? this._headingById.get(id) : null;
      return heading ? heading.getBoundingClientRect().top : Number.POSITIVE_INFINITY;
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
    this._topScopeElements.forEach((element) => {
      const elementScope =
        (element.dataset && element.dataset.outlineScope) ||
        element.getAttribute("data-outline-scope");
      element.classList.toggle("active-scope", elementScope === activeScope);
    });
  }

  _smoothScrollToId(id) {
    const heading = this._headingById.get(id);
    if (!heading) return;

    const header = document.querySelector("header");
    const headerHeight = header ? header.getBoundingClientRect().height : 52;
    const unclampedTarget = Math.max(
      0,
      heading.getBoundingClientRect().top + window.scrollY - headerHeight - 8,
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
