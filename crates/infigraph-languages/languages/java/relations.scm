; Java relationship extraction queries

; Method invocations on objects: obj.method()
(method_invocation
  object: (_) @call.receiver
  name: (identifier) @call.func) @call.site

; Unqualified method invocations: method()
(method_invocation
  !object
  name: (identifier) @call.func) @call.site

; Object creation: new Foo()
(object_creation_expression
  type: (type_identifier) @call.func) @call.site

; Import declarations
(import_declaration
  (scoped_identifier) @import.module)

; Class inheritance: extends
(class_declaration
  name: (identifier) @inherit.child
  (superclass
    (type_identifier) @inherit.parent))

; Interface implementation: implements
(class_declaration
  name: (identifier) @inherit.child
  (super_interfaces
    (type_list
      (type_identifier) @inherit.parent)))
