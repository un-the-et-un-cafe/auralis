package chat.omcp.app.mesh

/** One reconnect recovery and one storage-error recovery per link; never per chunk/flush. */
internal class ReceiptRecoveryGate {
    private var establishedSeen = false
    private var storageErrorSeen = false
    fun observe(established: Boolean, storageError: Boolean): Boolean {
        val recover = (established && !establishedSeen) || (storageError && !storageErrorSeen)
        establishedSeen = establishedSeen || established
        storageErrorSeen = storageErrorSeen || storageError
        return recover
    }
}
