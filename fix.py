import re

css = open("src/server/assets/style.css").read()

replacements = [
    (r"blockquote\.markdown-alert-title {\s+display: flex;\s+font-weight: 500;\s+align-items: center;\s+line-height: 1;\s+margin-bottom: 8px;\s+}\s+blockquote\.markdown-alert-title svg {\s+margin-right: 8px;\s+fill: currentColor;\s+}",
     r"blockquote[class^=\"markdown-alert-\"]::before {\n    display: block;\n    font-weight: 500;\n    line-height: 1;\n    margin-bottom: 8px;\n}"),
    (r"blockquote\.markdown-alert-note {\s+border-left-color: #0969da;\s+}\s+blockquote\.markdown-alert-note \.markdown-alert-title {\s+color: #0969da;\s+}",
     r"blockquote.markdown-alert-note {\n    border-left-color: #0969da;\n    background-color: rgba(9, 105, 218, 0.05);\n}\nblockquote.markdown-alert-note::before {\n    content: \"ℹ️ Note\";\n    color: #0969da;\n}"),
    (r"blockquote\.markdown-alert-tip {\s+border-left-color: #1a7f37;\s+}\s+blockquote\.markdown-alert-tip \.markdown-alert-title {\s+color: #1a7f37;\s+}",
     r"blockquote.markdown-alert-tip {\n    border-left-color: #1a7f37;\n    background-color: rgba(26, 127, 55, 0.05);\n}\nblockquote.markdown-alert-tip::before {\n    content: \"💡 Tip\";\n    color: #1a7f37;\n}"),
    (r"blockquote\.markdown-alert-important {\s+border-left-color: #8250df;\s+}\s+blockquote\.markdown-alert-important \.markdown-alert-title {\s+color: #8250df;\s+}",
     r"blockquote.markdown-alert-important {\n    border-left-color: #8250df;\n    background-color: rgba(130, 80, 223, 0.05);\n}\nblockquote.markdown-alert-important::before {\n    content: \"⚠️ Important\";\n    color: #8250df;\n}"),
    (r"blockquote\.markdown-alert-warning {\s+border-left-color: #9a6700;\s+}\s+blockquote\.markdown-alert-warning \.markdown-alert-title {\s+color: #9a6700;\s+}",
     r"blockquote.markdown-alert-warning {\n    border-left-color: #9a6700;\n    background-color: rgba(154, 103, 0, 0.05);\n}\nblockquote.markdown-alert-warning::before {\n    content: \"🚧 Warning\";\n    color: #9a6700;\n}"),
    (r"blockquote\.markdown-alert-caution {\s+border-left-color: #cf222e;\s+}\s+blockquote\.markdown-alert-caution \.markdown-alert-title {\s+color: #cf222e;\s+}",
     r"blockquote.markdown-alert-caution {\n    border-left-color: #cf222e;\n    background-color: rgba(207, 34, 46, 0.05);\n}\nblockquote.markdown-alert-caution::before {\n    content: \"🛑 Caution\";\n    color: #cf222e;\n}"),
]

for old, new in replacements:
    css = re.sub(old, new, css)

with open("src/server/assets/style.css", "w") as f:
    f.write(css)

