((comment) @doc
  .
  (comment)* @doc.run
  .
  [
    (_) @documented
    (export_statement (_) @documented)
    (_ (variable_declarator) @documented)
    (export_statement (_ (variable_declarator) @documented))
  ]
  (#match? @doc "^/\\*\\*")
  (#match? @doc.run "^/\\*\\*"))
