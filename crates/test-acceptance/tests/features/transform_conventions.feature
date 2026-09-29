# Canonical -> TMS unit scaling and sign conventions, MIL-STD-3071 §2.7.
# Compliance: docs/tms-compliance.md §4 ("Source mapping and deviations"); implementation at
# lib-mapping/src/transform.rs:7-12 (module doc) and transform.rs's `scale` module.
#
# These two conventions are load-bearing for every field the adapter emits:
#   * Sign inversion — Seplos reports positive current as CHARGING (into the battery); TMS
#     positive is SOURCING (into the microgrid). So tms_amperage = -vendor_current.
#   * Unit scaling — centi-volts/amps -> V/A (/100), millivolts -> V (/1000),
#     raw Kelvin x10 -> degC ((raw-2731)/10), deci-percent -> per-unit 0..1 (/1000).

Feature: Canonical -> TMS unit scaling and sign conventions

  Scenario Outline: Centi-volts scale to volts (scale::cv_to_v)
    Then <raw> centi-volts scales to approximately <volts> V via cv_to_v

    Examples:
      | raw  | volts |
      | 5452 | 54.52 |
      | 5451 | 54.51 |

  Scenario Outline: Millivolts scale to volts (scale::mv_to_v)
    Then <raw> millivolts scales to approximately <volts> V via mv_to_v

    Examples:
      | raw  | volts |
      | 3894 | 3.894 |
      | 3893 | 3.893 |
      | 3895 | 3.895 |

  Scenario Outline: Raw Kelvin x10 scales to degrees Celsius (scale::dk_to_c)
    Then <raw> raw Kelvin x10 scales to approximately <celsius> degC via dk_to_c

    Examples:
      | raw  | celsius |
      | 2801 | 7.0     |
      | 2811 | 8.0     |

  Scenario Outline: Deci-percent scales to per-unit (scale::dpct_to_pu)
    Then <raw> deci-percent scales to approximately <per_unit> per-unit via dpct_to_pu

    Examples:
      | raw | per_unit |
      | 775 | 0.775    |

  Scenario Outline: Vendor charge-positive current inverts to TMS source-positive amperage
    Given a canonical reading with pack current <vendor_ca> centi-amps
    When the reading is mapped to a DC measurement
    Then the DC line amperage is approximately <tms_a> A

    Examples:
      | vendor_ca | tms_a |
      | -164      | 1.64  |
      | -144      | 1.44  |
      | 500       | -5.00 |
