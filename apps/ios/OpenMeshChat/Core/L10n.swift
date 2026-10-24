import Foundation

/// Localized string (en / fr in Resources/*.lproj/Localizable.strings).
func L(_ key: String) -> String { NSLocalizedString(key, comment: "") }
func L(_ key: String, _ args: CVarArg...) -> String { String(format: NSLocalizedString(key, comment: ""), arguments: args) }
