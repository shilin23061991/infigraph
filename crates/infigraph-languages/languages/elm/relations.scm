; Elm relationship extraction queries

; Function application: functionName arg1 arg2
(function_call_expr
  target: (value_expr
    (value_qid
      (lower_case_identifier) @call.func))) @call.site

; Qualified function application: Module.func
(function_call_expr
  target: (value_expr
    (value_qid
      (upper_case_identifier) @call.receiver
      (lower_case_identifier) @call.func))) @call.site

; Import declarations
(import_clause
  (upper_case_qid
    (upper_case_identifier) @import.module))

; Exposing type references (tracks what's imported from modules)
(exposed_type
  (upper_case_identifier) @import.module)
