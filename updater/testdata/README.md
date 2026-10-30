The release fixture is the public v0.1.19 SHA256SUMS manifest and its original
Sigstore signature from ItsAnunesS/elo-talks. The legacy cosign JSON envelope was
converted to standard Sigstore bundle v0.1 fields without changing the signed
manifest, certificate, signature, Rekor canonicalized body, or inclusion promise.
It tests the real workflow identity and cryptographic verification, not a mock.

The network integration test fetches authenticated Sigstore TUF trust material.
Enable it with ELO_UPDATER_NETWORK_TEST=1; the helper preparation step always does.
