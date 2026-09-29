# Regression guards on the shape of tms-coverage.json itself -- independent of any specific
# topic's classification, these catch the descriptor factory (tools/tms-idl-codegen) silently
# losing coverage it used to have.
package main

import rego.v1

total_topic_bindings := sum([count(t.topics) |
	some t in input.types
])

# The catalogue's total topic-binding count must not fall below the MIL-STD-3071 Table B.2
# topic count (37, as of the 2.0.2 IDL) -- a drop means a topic got dropped from the generated
# catalogue without anyone noticing (e.g. an IDL parser regression, or someone hand-editing the
# `@generated` file, which the file's own header says never to do).
deny contains msg if {
	total_topic_bindings < data.tms_topic_classes.min_topic_bindings
	msg := sprintf(
		"Total topic bindings in tms-coverage.json (%d) fell below the MIL-STD-3071 Table B.2 baseline (%d) -- a topic binding was lost",
		[total_topic_bindings, data.tms_topic_classes.min_topic_bindings],
	)
}

# No duplicate (module, name) pairs -- would indicate a parser/model bug in the descriptor
# factory (two structs claiming the same qualified type name).
deny contains msg if {
	some m in {t.module | some t in input.types}
	names := [t.name | some t in input.types; t.module == m]
	count(names) != count({n | some n in names})
	msg := sprintf("Duplicate type name(s) detected within module %q", [m])
}
