package chat.omcp.app

import android.app.Application
import chat.omcp.app.backup.CloudBackupStore
import chat.omcp.app.backup.DriveAppDataBackupStore
import chat.omcp.app.backup.InMemoryCloudBackupStore
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.data.PrefsAppStore

class OmcpApplication : Application() {
    lateinit var repository: MeshRepository
        private set

    /**
     * Google Drive appDataFolder store. No OAuth client ships in this repo, so release
     * builds get the unavailable stub; debug builds may opt into an in-memory simulation
     * (BuildConfig.SIMULATED_DRIVE_BACKUP) to exercise the flow.
     */
    val cloudBackup: CloudBackupStore by lazy {
        if (BuildConfig.SIMULATED_DRIVE_BACKUP) InMemoryCloudBackupStore() else DriveAppDataBackupStore()
    }

    override fun onCreate() {
        super.onCreate()
        repository = MeshRepository(RustOmcpCore(), PrefsAppStore(this))
        runCatching { repository.restore() }
    }
}
