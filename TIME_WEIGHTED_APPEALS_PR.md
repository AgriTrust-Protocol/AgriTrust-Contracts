# Time-Weighted Voting for Grant Appeals

## Summary
This PR implements a specialized appeal mechanism for cancelled grants that uses time-weighted voting to give more influence to long-term token holders ("Wise Elders" of the community).

## Problem Solved
When grants are cancelled, grantees need a fair appeals process. This implementation ensures that appeals are judged by experienced community members who have demonstrated long-term commitment through sustained token holdings, rather than being influenced by short-term participants or coordinated attacks.

## Key Features

### Time-Weighted Voting System
- **25% weight** for new holders (< 30 days)
- **50% weight** for medium-term holders (30+ days)  
- **75% weight** for established holders (90+ days)
- **90% weight** for long-term holders (180+ days)
- **100% weight** for veteran holders (365+ days)
- **120% weight** for legacy holders (730+ days) - bonus for extreme loyalty

### Appeal Process
- **7-day voting period** with transparent deadlines
- **10% minimum participation** requirement
- **66% super-majority approval** threshold
- **Evidence hashing** for documentation integrity
- **Duplicate prevention** (one active appeal per grant)

### Smart Contract Implementation
- `GrantAppealContract` with full time-weighted voting logic
- Token holding duration tracking and caching
- Comprehensive error handling and validation
- Integration with existing governance structure

## Technical Details

### Core Components
1. **GrantAppeal**: Appeal data structure with voting tallies
2. **TimeWeightedVote**: Individual vote with time multiplier applied
3. **TokenHoldingInfo**: Tracks acquisition dates for duration calculation
4. **AppealDataKey**: Storage keys for efficient data access

### Key Functions
- `create_appeal()`: Submit appeal for cancelled grant
- `vote_on_appeal()`: Cast time-weighted vote
- `execute_appeal()`: Process successful appeal results
- `calculate_time_weighted_voting_power()`: Core voting power calculation

### Safety Features
- Prevention of duplicate voting
- Automatic voting period enforcement
- Participation and approval threshold validation
- Evidence integrity verification through hashing

## Testing
Comprehensive test suite covering:
- Time multiplier calculations
- Appeal creation and validation
- Voting mechanics and power calculations
- Appeal execution with threshold validation
- Edge cases and error conditions

## Integration
- Added as `grant_appeals` module to existing contract structure
- Compatible with current governance token system
- Follows established patterns from `governance.rs`
- Maintains consistency with existing error handling

## Benefits
1. **Experience-based governance**: Long-term stakeholders have more say
2. **Resistance to manipulation**: Short-term coordinated attacks have reduced impact
3. **Fair appeals process**: Structured mechanism for grant reinstatement
4. **Community wisdom**: Leverages "skin in the game" principle
5. **Transparent process**: All voting data and calculations are on-chain

## Security Considerations
- Time-weight calculations use deterministic algorithms
- Token holding info is updated on each vote to maintain accuracy
- All voting periods are enforced by ledger timestamps
- Evidence is stored as hashes to prevent data bloat

This implementation provides a robust, fair, and secure appeals mechanism that prioritizes the wisdom of long-term community members while maintaining accessibility for newer participants.
