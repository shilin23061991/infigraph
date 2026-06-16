; Svelte relationship extraction queries
; Note: Svelte uses tree-sitter-svelte-ng which parses the template layer.
; Embedded JS/TS in <script> blocks requires language injection for full
; call/import extraction. These patterns capture what the Svelte grammar exposes.

; Component references in template: <Component />
(element
  (self_closing_tag
    (tag_name) @call.func)) @call.site

; Component references in template: <Component>...</Component>
(element
  (start_tag
    (tag_name) @call.func)) @call.site
