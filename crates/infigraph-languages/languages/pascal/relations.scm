; Pascal / Delphi relation extraction queries

; Function / procedure calls — direct: Foo()
(exprCall
  entity: (identifier) @call.func) @call.site

; Method calls — dotted: Obj.Method() or Unit.Proc()
(exprCall
  entity: (exprDot
    lhs: (_) @call.receiver
    rhs: (identifier) @call.func)) @call.site

; Inherited method calls: inherited Create, inherited Destroy, etc.
(inherited
  (identifier) @call.func) @call.site

; Uses clause — imports
(declUses
  (moduleName
    (identifier) @import.module))

; Class inheritance: TDerived = class(TBase)
(declType
  name: (identifier) @inherit.child
  type: (declClass
    parent: (typeref
      (identifier) @inherit.parent)))

; Interface inheritance: IMyIntf = interface(IParent)
(declType
  name: (identifier) @inherit.child
  type: (declIntf
    parent: (typeref
      (identifier) @inherit.parent)))

; Class/record helper: THelper = class helper for TTarget
(declType
  name: (identifier) @inherit.child
  type: (declHelper
    (typeref
      (identifier) @inherit.parent)))
