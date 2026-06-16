; Clojure relationship extraction queries

; Function calls: (func-name args...)
; In Clojure, the first element of a list is the function being called
(list_lit
  value: (sym_lit) @call.func) @call.site

; Namespace requires: (ns foo (:require [bar.baz]))
; Keyword :require/:import in ns form
(list_lit
  value: (kwd_lit) @_kw
  (#match? @_kw "^:(require|import|use)$")
  value: (vec_lit
    value: (sym_lit) @import.module))

; Simple require: (require '[foo.bar])
(list_lit
  value: (sym_lit) @_req
  (#match? @_req "^(require|import|use)$")
  value: (sym_lit) @import.module)
