# tms::dc::MeasurementUpdate — IDL `chapter-d.txt` lines 13882-13926.
# Compliance: docs/tms-compliance.md §3.1 ("tms::dc::MeasurementUpdate -> DcMeasurementUpdate").
# The sign-inversion / unit-scaling matrix lives in transform_conventions.feature; this file
# covers the DcMeasurementUpdate struct SHAPE (port/line cardinality) using the known-value
# fixture shared with lib-mapping/tests/tms_transform.rs.

Feature: DcMeasurementUpdate field correctness

  Scenario: A discharging pack maps to exactly one external DC line with no internal measurement
    Given the tms_transform sample reading (3 cells, 2 temps, discharging 1.44 A)
    When the reading is mapped to a DC measurement
    Then there is 1 external measurement port
    And the internal measurement is empty
    And the DC line voltage is approximately 54.51 V
    And the DC line amperage is approximately 1.44 A
    And the DC line power is approximately 78.4944 W
