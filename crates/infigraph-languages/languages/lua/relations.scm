; Lua relationship extraction queries

; Function calls
(function_call
  name: (identifier) @call.func) @call.site

; Method calls: obj.method()
(function_call
  name: (dot_index_expression
    table: (_) @call.receiver
    field: (identifier) @call.func)) @call.site

; Method calls: obj:method()
(function_call
  name: (method_index_expression
    table: (_) @call.receiver
    method: (identifier) @call.func)) @call.site
