; Ruby relationship extraction queries

; Method calls: obj.method()
(call
  receiver: (_) @call.receiver
  method: (identifier) @call.func) @call.site

; Unqualified method calls
(call
  !receiver
  method: (identifier) @call.func) @call.site

; Class inheritance: class Foo < Bar
(class
  name: (constant) @inherit.child
  (superclass
    (constant) @inherit.parent))
