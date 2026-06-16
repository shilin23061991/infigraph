; Kotlin relationship extraction queries

; Simple function calls: funcName()
(call_expression
  (identifier) @call.func) @call.site

; Method calls: obj.method()
(call_expression
  (navigation_expression
    (_) @call.receiver
    (identifier) @call.func)) @call.site

; Import declarations
(import
  (identifier) @import.module)
