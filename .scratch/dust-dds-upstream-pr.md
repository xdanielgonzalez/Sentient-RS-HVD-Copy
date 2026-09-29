# Upstream PR draft — `dust-dds`: emit/consume the rule-(12) NEXTINT for XCDR2 sequences of non-primitive elements

**Target:** https://github.com/s2e-systems/dust-dds (base `main`; the change is against the 0.15.0
`src/xtypes/{serializer,deserializer}.rs`).
**DO NOT open automatically** — this is a branch-ready draft for a human to submit.

Suggested branch: `xcdr2-sequence-nextint-rule12`

---

## Title

`fix(xtypes): emit + consume the DHEADER/NEXTINT for XCDR2 sequences of non-primitive elements (DDS-XTypes 1.3 rule (12))`

## PR body

### Summary

In XCDR2 (encoding version 2), a sequence whose **element type is non-primitive** (struct or string
elements) must be serialized with a leading **NEXTINT** — a 4-byte `UInt32` DHEADER giving the byte
length of `{ length + elements }`:

```
rule (11) primitive element seq:      { length:u32 } << element*
rule (12) NON-primitive element seq:  { NEXTINT:u32 } << { length:u32 } << element*
```

This is **DDS-XTypes 1.3 §7.4.3.5.3 rule (12)** (OMG `formal/2020-02-04`). `dust_dds` 0.15.0's
`Xcdr2Serializer` emits the rule-(11) form for *all* element types, omitting the NEXTINT for
non-primitive elements. Its deserializer likewise does not expect one.

### Why it matters (interop)

**Eclipse CycloneDDS (11.0.1) requires the NEXTINT and its decoder rejects a non-primitive sequence
without it.** Observed directly: a CycloneDDS subscriber fails to deserialize a struct carrying a
`sequence<SomeStruct>` (`deserialization <Type> failed`), while a struct with only primitive
sequences / scalars decodes cleanly. So `dust_dds`-serialized XCDR2 is currently un-decodable by
CycloneDDS for any type containing a struct-or-string sequence — a common shape (e.g. a
`sequence<Measurement>`).

This is a known, live interoperability question:

- eclipse-cyclonedds / **cyclonedds-cxx#218** — CycloneDDS emits/requires the collection delimiter.
- OMG DDS-XTypes erratum **DDSXTY14-56** — clarifies the rule-(12) NEXTINT for collections of
  non-primitive elements.

CycloneDDS is a reference DDS implementation, and the delimiter is what the normative rule (12)
prescribes, so aligning `dust_dds` to emit + consume it improves interop without changing the
primitive-sequence, CDR1, or PL_CDR wire forms.

### The change (XCDR2-only, no change to CDR1 / PL_CDR)

Confined to the `EncodingVersion2` / `CdrVersion2` code paths so RTPS discovery (PL_CDR) and CDR1
output are byte-identical.

**Serializer** (`src/xtypes/serializer.rs`): the two non-primitive arms of the shared default
`serialize_sequence` (`STRING8`, `STRUCTURE`) are refactored to call two new overridable trait
methods whose default bodies reproduce the current `{ length } << element*` form exactly. The
`Xcdr2Serializer` impl overrides both to prepend the NEXTINT — its byte length is measured with a
fresh position-0 counting serializer (`ByteCounter`), which is correct because XCDR2 max-align is 4
and the NEXTINT is a 4-byte `u32`, so the counted subtree begins 4-aligned exactly as in the live
stream (the same position-independence argument the existing `serialize_appendable_struct` DHEADER
relies on). The counter uses `LittleEndian` unconditionally because a byte *count* is
endianness-independent (mirrors the existing `count_bytes_cdr2`).

**Deserializer** (`src/xtypes/deserializer.rs`): a new overridable `consume_collection_dheader`
(default no-op) is called at the top of `deserialize_string_sequence` and
`deserialize_complex_sequence`; `Cdr2Deserializer` overrides it to read (and discard) the 4-byte
NEXTINT. CDR1 / PL_CDR keep the no-op default.

Net effect: `dust_dds` round-trips its own XCDR2 bytes with the delimiter, and its `Cdr2LeSerializer`
output becomes byte-identical to a CycloneDDS dump for sequence-bearing types.

### Testing

- Existing `serialize_*` / round-trip unit tests still pass (primitive sequences, CDR1, PL_CDR, and
  the appendable-struct DHEADER paths are byte-unchanged).
- Verified end-to-end against **CycloneDDS 11.0.1** via a docker-compose conformance harness: a
  `sequence<struct>`-bearing topic that previously failed CycloneDDS deserialization now decodes.
- Suggested addition: a unit test asserting a `Vec<SomeAppendableStruct>` and a `Vec<String>` member
  serialize with the leading 4-byte length and round-trip through the deserializer (a
  `serialize_v2_le` golden analogous to the existing `serialize_string_list` test, now prefixed by
  the NEXTINT).

### Notes for maintainers

If you prefer to gate this behind the rule's exact "non-primitive" definition (structs, strings,
enums, nested sequences/arrays/maps), the same two override points extend naturally — this PR covers
the two cases `dust_dds` serializes today (`STRING8`, `STRUCTURE`). Happy to adjust the approach
(e.g. thread a `const` through the `CdrVersion` trait instead of the override methods) to match your
preferred style.

---

## Branch-ready diff

> The vendored copy in this repo fences the edits with `// --- SENTIENT rule-(12) patch ---`
> markers; for the upstream PR those markers/`vendor/dust_dds/PATCHES.md` references should be
> replaced with neutral comments citing DDS-XTypes 1.3 §7.4.3.5.3 rule (12). Otherwise the hunks are
> exactly as below.

### `src/xtypes/serializer.rs`

```diff
@@ trait XTypesSerializer { fn serialize_string(..) .. }
     fn serialize_string(&mut self, v: &String) {
         self.serialize_primitive_type(&(v.len() as u32 + 1));
         self.serialize_primitive_type(&v.as_bytes());
         self.serialize_primitive_type(&0u8);
     }
+
+    // Rule (12): a sequence of NON-primitive elements. Default (CDR1 / PL_CDR) = plain
+    // `{ length } << element*`; the XCDR2 impl overrides these to prepend the NEXTINT delimiter.
+    fn serialize_string_sequence(&mut self, list: &[String]) {
+        self.serialize_primitive_type(&(list.len() as u32));
+        for v in list {
+            self.serialize_string(v);
+        }
+    }
+    fn serialize_complex_sequence(&mut self, list: &[DynamicData]) -> Result<(), XTypesError> {
+        self.serialize_primitive_type(&(list.len() as u32));
+        for v in list {
+            self.serialize_complex(v)?;
+        }
+        Ok(())
+    }

@@ fn serialize_sequence(..) STRING8 / STRUCTURE arms
-            TypeKind::STRING8 => {
-                let list = v.get_string_values(member_id)?;
-                self.serialize_primitive_type(&(list.len() as u32));
-                for v in list {
-                    self.serialize_string(v);
-                }
-            }
+            TypeKind::STRING8 => self.serialize_string_sequence(v.get_string_values(member_id)?),
             ...
-            TypeKind::STRUCTURE => {
-                let list = v.get_complex_values(member_id)?;
-                self.serialize_primitive_type(&(list.len() as u32));
-                for v in list {
-                    self.serialize_complex(v)?;
-                }
-            }
+            TypeKind::STRUCTURE => self.serialize_complex_sequence(v.get_complex_values(member_id)?)?,

@@ impl XTypesSerializer for Xcdr2Serializer (after serialize_mutable_struct)
+    // Rule (12) NEXTINT: byte length of `{ length + elements }`, position-independent (XCDR2
+    // max-align 4, delimiter is a 4-byte u32 → subtree begins 4-aligned as in the live stream;
+    // same argument as serialize_appendable_struct's DHEADER). Count is endianness-independent.
+    fn serialize_string_sequence(&mut self, list: &[String]) {
+        let mut byte_counter = ByteCounter::new();
+        let mut counter = Xcdr2Serializer {
+            writer: CdrWriter::new(&mut byte_counter, LittleEndian, EncodingVersion2),
+        };
+        counter.serialize_primitive_type(&(list.len() as u32));
+        for v in list { counter.serialize_string(v); }
+        (byte_counter.0 as u32).serialize(&mut self.writer);
+        self.serialize_primitive_type(&(list.len() as u32));
+        for v in list { self.serialize_string(v); }
+    }
+    fn serialize_complex_sequence(&mut self, list: &[DynamicData]) -> Result<(), XTypesError> {
+        let mut byte_counter = ByteCounter::new();
+        let mut counter = Xcdr2Serializer {
+            writer: CdrWriter::new(&mut byte_counter, LittleEndian, EncodingVersion2),
+        };
+        counter.serialize_primitive_type(&(list.len() as u32));
+        for v in list { counter.serialize_complex(v)?; }
+        (byte_counter.0 as u32).serialize(&mut self.writer);
+        self.serialize_primitive_type(&(list.len() as u32));
+        for v in list { self.serialize_complex(v)?; }
+        Ok(())
+    }
```

### `src/xtypes/deserializer.rs`

```diff
@@ trait XTypesDeserialize (after deserialize_string)
+    // Consume the rule-(12) NEXTINT that precedes a non-primitive-element sequence in XCDR2.
+    // Default (CDR1 / PL_CDR): nothing on the wire. Cdr2Deserializer overrides to read the u32.
+    fn consume_collection_dheader(&mut self) -> XTypesResult<()> {
+        Ok(())
+    }

@@ fn deserialize_string_sequence
     fn deserialize_string_sequence(&mut self) -> XTypesResult<Vec<String>> {
+        self.consume_collection_dheader()?;
         let mut sequence_of_string = Vec::new();

@@ fn deserialize_complex_sequence
     ) -> XTypesResult<Vec<DynamicData>> {
+        self.consume_collection_dheader()?;
         let mut sequence_of_dynamic_data = Vec::new();

@@ impl XTypesDeserialize for Cdr2Deserializer (after deserialize_appendable_struct)
+    fn consume_collection_dheader(&mut self) -> XTypesResult<()> {
+        let _nextint = self.deserialize_primitive_type::<u32>()?;
+        Ok(())
+    }
```
