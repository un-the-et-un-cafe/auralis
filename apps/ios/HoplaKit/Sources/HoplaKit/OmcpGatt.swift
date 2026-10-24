/// BLE GATT layout, identical to Android (`apps/android/.../core/OmcpCore.kt` › `OmcpGatt`).
/// PROTOCOL §8.1's placeholder UUID is not valid hex, so the beta uses `4f4d4350-…`
/// ("OMCP" in ASCII hex). Finalize before ship. A unit test cross-checks the Kotlin file.
public enum OmcpGatt {
    public static let service = "4f4d4350-0001-4c01-9e00-000000000001"
    /// Central writes PDUs here (write without response).
    public static let inbox = "4f4d4350-0002-4c01-9e00-000000000001"
    /// Peripheral notifies PDUs here.
    public static let outbox = "4f4d4350-0003-4c01-9e00-000000000001"
    public static let control = "4f4d4350-0004-4c01-9e00-000000000001"
    public static let cccd = "00002902-0000-1000-8000-00805f9b34fb"
    /// Manufacturer-data company id for the 13-byte advertisement (0xFFFF = testing).
    public static let manufacturerId: UInt16 = 0xFFFF
}
