# Provenance gate: every type carrying at least one topic binding must trace back to a real,
# positive line number in the checked-in MIL-STD-3071 IDL (docs/compliance/tms/mil-std-3071_data_model.idl).
# This is the machine-readable operationalization of "every implemented type carries an
# idl_line provenance" from the WS-R mission brief -- a topic type with no traceable IDL line
# cannot be trusted as spec-derived (it could be a hand invention, exactly the class of bug
# issue #257's invented `Request` topic was).
package main

import rego.v1

deny contains msg if {
	some t in input.types
	count(t.topics) > 0
	not has_provenance(t)
	msg := sprintf(
		"Type %q (module %s) is bound to topic(s) %v but has no positive idl_line provenance (idl_line=%v)",
		[t.name, t.module, t.topics, object.get(t, "idl_line", null)],
	)
}

has_provenance(t) if {
	is_number(t.idl_line)
	t.idl_line > 0
}
