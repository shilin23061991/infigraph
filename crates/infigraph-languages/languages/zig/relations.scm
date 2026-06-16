; Zig relationship extraction queries

; Function calls
(call_expression
  function: (identifier) @call.func) @call.site

; Method calls: obj.method()
(call_expression
  function: (field_expression
    object: (_) @call.receiver
    member: (identifier) @call.func)) @call.site
