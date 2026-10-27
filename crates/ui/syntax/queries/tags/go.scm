(function_declaration name: (identifier) @name) @definition.function
(method_declaration name: (field_identifier) @name) @definition.method
(type_spec name: (type_identifier) @name) @definition.type
(type_alias name: (type_identifier) @name) @definition.type
(const_spec name: (identifier) @name) @definition.constant

(call_expression function: (identifier) @name) @reference.call
(call_expression function: (selector_expression field: (field_identifier) @name)) @reference.call
(type_identifier) @name @reference.type

((comment)+ @doc
  .
  [
    (_) @documented
    (type_declaration [(type_spec) (type_alias)] @documented)
    (const_declaration (const_spec) @documented)
  ])
