"""Verify the keyring operation recorded by a test-only secret-tool spy.

Run the client with isolated XDG directories and the spy first on PATH. Type
ux-demo, activate Forget me, then pass the spy's one-argument-per-line log here.
The spy must never delegate to the real keyring or record real credentials.
"""
import sys
from pathlib import Path

arguments = Path(sys.argv[1]).read_text().splitlines()
assert arguments[-5:] == ["clear", "service", "escargot-native", "account", "ux-demo@escargot.chat"], (
    "Forget me must clear the entered account, not an empty account"
)
print("Forget me clears the correct keyring account")
