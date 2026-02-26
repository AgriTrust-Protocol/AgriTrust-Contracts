if grantee.is_contract() {
    require_cross_contract_auth(grantee, caller)?;
} else {
    grantee.require_auth(); // normal account
}