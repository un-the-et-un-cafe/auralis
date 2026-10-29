package chat.omcp.app

import androidx.test.uiautomator.StaleObjectException
import org.junit.Assert.*
import org.junit.Test

/** Deterministic permission-helper checks without altering real app smoke state. */
class PermissionDialogAssertionTest {
    @Test fun staleHandleIsReacquiredWithoutExtendingDeadline() {
        var now = 0L
        val remaining = mutableListOf<Long>()
        val clicked = mutableListOf<Int>()
        var handle = 0
        assertTrue(clickFreshPermissionButton(1_000, { now }, {
            remaining += it
            ++handle
        }, {
            clicked += it
            now += 100
            if (it < 3) throw StaleObjectException()
        }))
        assertEquals(listOf(1, 2, 3), clicked)
        assertEquals(listOf(1_000L, 900L, 800L), remaining)
    }

    @Test fun continuousStalenessStopsAtOriginalDeadline() {
        var now = 0L
        var attempts = 0
        assertFalse(clickFreshPermissionButton(1_000, { now }, { Any() }, {
            attempts++
            now += 250
            throw StaleObjectException()
        }))
        assertEquals(4, attempts)
        assertEquals(1_000L, now)
    }

    @Test fun absentDialogDoesNotCountAsDenied() {
        assertFalse(clickFreshPermissionButton<Any>(1_000, { 0L }, { null }, {
            fail("Must not click a missing dialog")
        }))
    }

    @Test fun otherFailuresAreNotSwallowed() {
        val failure = IllegalStateException("real click failure")
        val actual = assertThrows(IllegalStateException::class.java) {
            clickFreshPermissionButton(1_000, { 0L }, { Any() }, { throw failure })
        }
        assertSame(failure, actual)
    }
}
