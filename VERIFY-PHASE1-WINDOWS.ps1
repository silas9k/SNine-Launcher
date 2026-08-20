$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Command,

        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$Arguments
    )

    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE`: $Command $($Arguments -join ' ')"
    }
}

$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ProjectRoot

Remove-Item -Recurse -Force node_modules, dist, "src-tauri\target" -ErrorAction SilentlyContinue

Invoke-Native npm ci
Invoke-Native npm test
Invoke-Native npm run build

Push-Location src-tauri
try {
    Invoke-Native cargo fmt --all -- --check
    Invoke-Native cargo check --locked
    Invoke-Native cargo clippy --locked --all-targets -- -D warnings

    Invoke-Native cargo test --locked operation_plan_preflight_enforces_the_real_root_budget_before_journaling -- --nocapture
    Invoke-Native cargo test --locked classifies_verified_windows_junctions_with_the_stable_reparse_error -- --nocapture
    Invoke-Native cargo test --locked rejects_windows_directory_junctions_after_verified_fixture_creation -- --nocapture
    Invoke-Native cargo test --locked -- --nocapture

    Invoke-Native cargo test --locked crash_recovery_never_leaves_a_mixed_revision -- --nocapture
    Invoke-Native cargo test --locked rejects_existing_hardlinks -- --nocapture
    Invoke-Native cargo test --locked accepts_path_at_the_available_relative_boundary -- --nocapture
    Invoke-Native cargo test --locked rejects_path_one_unit_beyond_the_available_relative_boundary -- --nocapture
    Invoke-Native cargo test --locked absolute_path_budget_accounts_for_the_registered_root_length -- --nocapture
    Invoke-Native cargo test --locked generated_profile_and_staging_paths_fit_the_documented_budget -- --nocapture
    Invoke-Native cargo test --locked phase1_transaction_demo -- --nocapture
}
finally {
    Pop-Location
}

Invoke-Native npm run tauri:build

Write-Host "Phase 1 v1.0.3 verification completed. Any generated installer is unsigned and must not be published."
