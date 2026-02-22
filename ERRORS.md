# Error Codes Mapping

When interacting with the Grant Stream smart contracts, developers might encounter generic numerical error codes (e.g., `Error(7)`). This table maps these numerical codes to human-readable reasons to help with debugging.

| Error Code | Human-Readable Reason   | Description                                                                       |
| ---------- | ----------------------- | --------------------------------------------------------------------------------- |
| `1`        | Not Authorized          | The caller does not have the required permissions.                                |
| `2`        | Insufficient Balance    | The account or contract does not have enough funds to complete the transaction.   |
| `3`        | Grant Not Found         | The specified grant ID does not exist in storage.                                 |
| `4`        | Grant Paused            | The grant has been paused by the admin or council.                                |
| `5`        | Invalid Amount          | The specified amount is invalid (e.g., exceeds remaining balance or total grant). |
| `6`        | Already Exists          | The resource (grant, milestone, etc.) already exists.                             |
| `7`        | Under Dispute / Blocked | The action is blocked due to an active dispute or existing state.                 |

_Note: If you encounter an error code not listed here, please verify the contract source code or Soroban SDK standard errors._
