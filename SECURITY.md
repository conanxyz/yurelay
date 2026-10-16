# Security policy

Yurelay is currently a development preview; fixes target the latest source and
preview release. No stable maintenance branches are promised yet.

Report suspected vulnerabilities privately to **12594210+conanxyz@users.noreply.github.com**, or use GitHub
private vulnerability reporting when enabled. Include affected version, macOS
version, a synthetic reproduction and impact. Do not include live API keys,
private keys, seed phrases, captured private text, databases or screenshots with
personal data. Do not open a public issue containing exploit details before a fix.

SensitiveGuard is local, conservative pattern detection and may miss secrets or
produce false positives. Remote translation deliberately transmits approved text
to the selected provider. Keys stay in Keychain; local history is not an encrypted
vault. Review service trust and configure history retention for your environment.

Downloaded development builds are ad-hoc signed and not notarized. Check origin
and release checksums. Never remove quarantine from an untrusted binary.
