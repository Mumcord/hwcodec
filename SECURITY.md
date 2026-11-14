# Security policy

hwcodec is part of the Mumcord project. The [Mumcord security policy](https://github.com/Mumcord/mumcord/blob/main/SECURITY.md) applies to it in full: what to expect after a report, disclosure, and the rules for testing.

## Supported versions

Security fixes are made for the **latest release** and ship as a new patch release. Older releases do not get fixes; please update to the latest release.

## Reporting a vulnerability

**Do not report vulnerabilities in public issues, pull requests or discussions.**

Report privately through GitHub:

**<https://github.com/Mumcord/hwcodec/security/advisories/new>**

Only you and the maintainers can read the report. If you cannot use this form, open a normal issue that only says you would like to report a security problem and asks for a private channel. Do not include any details in it.

A good report contains the hwcodec version, your operating system, GPU and driver, what an attacker can achieve and what they need for it (for example: a crafted video stream handed to a decoder), and steps to reproduce.

## What counts as a vulnerability

- Memory safety problems reachable through the public interface, for example a crafted AV1 or VP9 stream that makes a decoder read or write out of bounds.
- A build that fetches or runs something it should not.

Problems in the bundled libraries themselves (SVT-AV1, libvpx, dav1d, libvpl) are best reported to their own projects as well; tell us too, so that a release with the fixed version can follow.
