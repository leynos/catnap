Feature: GNU-like sleep operands

  Scenario: Sum suffixed operands
    Given sleep operands "1m 5s"
    When the sleep command is parsed
    Then the parsed duration is 65 seconds

  Scenario: Reject invalid suffixes
    Given sleep operands "1w"
    When the sleep command is parsed
    Then the command error mentions "invalid time suffix"
