(struct_item name: (type_identifier) @name) @definition.type
(enum_item name: (type_identifier) @name) @definition.type
(union_item name: (type_identifier) @name) @definition.type
(type_item name: (type_identifier) @name) @definition.type
(trait_item name: (type_identifier) @name) @definition.interface
(mod_item name: (identifier) @name) @definition.module
(macro_definition name: (identifier) @name) @definition.macro
(const_item name: (identifier) @name) @definition.constant
(static_item name: (identifier) @name) @definition.constant
(declaration_list (function_item name: (identifier) @name) @definition.method)
(declaration_list (function_signature_item name: (identifier) @name) @definition.method)
(function_item name: (identifier) @name) @definition.function
(impl_item type: (type_identifier) @name) @definition.implementation
(impl_item type: (generic_type type: (type_identifier) @name)) @definition.implementation

(call_expression function: (identifier) @name) @reference.call
(call_expression function: (field_expression field: (field_identifier) @name)) @reference.call
(call_expression function: (scoped_identifier name: (identifier) @name)) @reference.call
(call_expression function: (generic_function function: (identifier) @name)) @reference.call
(call_expression function: (generic_function function: (field_expression field: (field_identifier) @name))) @reference.call
(call_expression function: (generic_function function: (scoped_identifier name: (identifier) @name))) @reference.call
(macro_invocation macro: (identifier) @name) @reference.call
((scoped_identifier path: (identifier) @name) @reference.type (#match? @name "^[A-Z]"))
(type_identifier) @name @reference.type

(_ (attributes [(line_outer_doc_comment) (block_outer_doc_comment)] @doc)) @documented
