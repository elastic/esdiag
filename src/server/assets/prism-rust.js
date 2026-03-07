Prism.languages.rust = {
  comment: [
    {
      pattern: /(^|[^\\])\/\*[\s\S]*?(?:\*\/|$)/,
      lookbehind: true,
      greedy: true,
    },
    {
      pattern: /(^|[^\\:])\/\/.*/,
      lookbehind: true,
      greedy: true,
    },
  ],
  string: {
    pattern: /(^|[^\\])"(?:\\.|[^"\\\r\n])*"/,
    lookbehind: true,
    greedy: true,
  },
  "byte-string": {
    pattern: /(^|[^\\])b"(?:\\.|[^"\\\r\n])*"/,
    lookbehind: true,
    greedy: true,
    alias: "string",
  },
  char: {
    pattern: /(^|[^\\])b?'(?:\\(?:x[0-7][\da-fA-F]|u\{[\da-fA-F]{1,6}\}|.)|[^\\'])'/,
    lookbehind: true,
    greedy: true,
  },
  attribute: {
    pattern: /#!?\[[^\]]+\]/,
    greedy: true,
    alias: "atrule",
  },
  lifetime: {
    pattern: /'\w+/,
    alias: "symbol",
  },
  macro: {
    pattern: /\b[a-z_]\w*!/,
    alias: "function",
  },
  keyword:
    /\b(?:as|async|await|break|const|continue|crate|dyn|else|enum|extern|fn|for|if|impl|in|let|loop|match|mod|move|mut|pub|ref|return|self|Self|static|struct|super|trait|type|union|unsafe|use|where|while)\b/,
  "class-name": {
    pattern: /\b[A-Z]\w*\b/,
    greedy: true,
  },
  function: /\b[a-z_]\w*(?=\s*(?:<[^<>]*(?:<[^<>]*>[^<>]*)*>)*\s*\()/,
  number:
    /\b(?:0x[\da-fA-F_]+|0o[0-7_]+|0b[01_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d[\d_]*)?)(?:_(?:[iu](?:8|16|32|64|128|size)|f(?:32|64)))?\b/,
  boolean: /\b(?:true|false)\b/,
  operator:
    />>=?|<<=?|->|=>|\.\.|[=!<>]=?|[-+*/%&|^]=?|&&|\|\||[?~]/,
  punctuation: /[{}[\];(),.:]/,
};
