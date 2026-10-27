(class_definition body: (block (function_definition name: (identifier) @name) @definition.method))
(class_definition body: (block (decorated_definition definition: (function_definition name: (identifier) @name) @definition.method)))
(class_definition name: (identifier) @name) @definition.type
(function_definition name: (identifier) @name) @definition.function

(call function: (identifier) @name) @reference.call
(call function: (attribute attribute: (identifier) @name)) @reference.call
(type (identifier) @name) @reference.type

(_ body: (block . (string (string_content) @doc))) @documented
