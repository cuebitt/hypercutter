"""Custom exception hierarchy for hypercutter."""


class HypercutterError(Exception):
    pass


class DecompressionError(HypercutterError):
    pass


class RomError(HypercutterError):
    pass


class SymbolError(HypercutterError):
    pass


class ExtractionError(HypercutterError):
    pass
