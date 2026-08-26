$files = @("contracts/invoice-escrow/src/test.rs", "contracts/invoice-escrow/src/integration_test.rs")

foreach ($file in $files) {
    $content = Get-Content -Raw $file
    $content = [regex]::Replace($content, '(&commitment,\s*(&None|&Some\([^)]+\)|&milestone)),\s*\)', "`$1,`r`n            &None,`r`n        )")
    Set-Content -Path $file -Value $content -NoNewline
}
