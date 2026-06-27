"""Custom exception hierarchy for hypercutter."""


class HypercutterError(Exception):
    """Base exception for all hypercutter errors."""

    pass


class DecompressionError(HypercutterError):
    """Raised when LZ77 decompression fails."""

    pass
