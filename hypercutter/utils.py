from typing import Any, TypeVar

T = TypeVar("T")


def find_by_field(items: list[T], field: str, value: Any) -> T | None:
    """
    Find the first item in a list that has an attribute matching the given value.

    Args:
        items: List of objects to search.
        field: Name of the attribute to check.
        value: Value to match against.

    Returns:
        The first matching item, or None if not found.
    """
    return next((item for item in items if getattr(item, field) == value), None)


def find_primary_from_secondary(
    pairs: dict[str, list[str]], secondary: str
) -> str | None:
    """
    Find the primary tileset name for a given secondary tileset.

    Args:
        pairs: Dictionary mapping primary tileset names to lists of secondary tileset names.
        secondary: The secondary tileset name to look up.

    Returns:
        The primary tileset name if found, or None.
    """
    if secondary in pairs:
        return secondary
    for primary, secondary_list in pairs.items():
        if secondary in secondary_list:
            return primary
    return None
