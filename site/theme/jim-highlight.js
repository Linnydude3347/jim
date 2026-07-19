// Registers a highlight.js language definition for jim and re-highlights any
// jim code blocks. mdBook's bundled highlight.js runs on DOMContentLoaded and
// doesn't know jim, so we register the grammar and re-run on `load` (which
// fires afterwards). If anything is missing, blocks degrade to plain monospace.
(function () {
  function jim(hljs) {
    return {
      name: "jim",
      case_insensitive: false,
      keywords: {
        keyword:
          "if else for while in break continue return try catch var const " +
          "function class public private and or not div",
        literal: "true false None",
        built_in:
          "Integer Float Bool Char String Exception Array Vector RawBuffer",
        variable: "this",
      },
      contains: [
        hljs.COMMENT("//", "$"),
        { className: "meta", begin: "#import", end: "$" },
        {
          className: "string",
          begin: '"',
          end: '"',
          contains: [{ begin: "\\\\." }],
        },
        {
          className: "string",
          begin: "'",
          end: "'",
          contains: [{ begin: "\\\\." }],
        },
        // intrinsics: @print_string, @f64_sqrt, ...
        { className: "symbol", begin: "@[A-Za-z_][A-Za-z0-9_]*" },
        { className: "number", begin: "\\b[0-9]+(\\.[0-9]+)?\\b" },
      ],
    };
  }

  function apply() {
    if (!window.hljs || typeof hljs.registerLanguage !== "function") return;
    try {
      hljs.registerLanguage("jim", jim);
    } catch (e) {
      /* already registered */
    }
    var blocks = document.querySelectorAll("code.language-jim");
    for (var i = 0; i < blocks.length; i++) {
      var b = blocks[i];
      b.removeAttribute("data-highlighted");
      b.classList.remove("hljs");
      if (typeof hljs.highlightElement === "function") {
        hljs.highlightElement(b);
      } else if (typeof hljs.highlightBlock === "function") {
        hljs.highlightBlock(b);
      }
    }
  }

  if (document.readyState === "complete") {
    apply();
  } else {
    window.addEventListener("load", apply);
  }
})();
