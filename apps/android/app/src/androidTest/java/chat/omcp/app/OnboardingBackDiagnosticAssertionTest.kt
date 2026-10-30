package chat.omcp.app

import androidx.compose.ui.semantics.SemanticsConfiguration
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.text.AnnotatedString
import org.junit.Assert.*
import org.junit.Test

/** Pure predicate coverage. No Activity, shell, input action or app identity changes. */
class OnboardingBackDiagnosticAssertionTest {
    @Test fun textAndEditableTextUseExactMatches() {
        val expected = "Synthetic fixture"
        val label = SemanticsConfiguration().apply {
            this[SemanticsProperties.Text] = listOf(AnnotatedString("Other"), AnnotatedString(expected))
        }
        val editor = SemanticsConfiguration().apply {
            this[SemanticsProperties.EditableText] = AnnotatedString(expected)
        }
        assertTrue(label.matchesText(expected))
        assertTrue(editor.matchesText(expected))
        assertFalse(label.matchesText("synthetic fixture"))
        assertFalse(editor.matchesText("Synthetic"))
        assertFalse(SemanticsConfiguration().matchesText(expected))
    }

    @Test fun fixtureMatchDoesNotConfuseLabelWithCommittedEditableValue() {
        val expected = "Synthetic fixture"
        val config = SemanticsConfiguration().apply {
            this[SemanticsProperties.Text] = listOf(AnnotatedString(expected))
            this[SemanticsProperties.EditableText] = AnnotatedString("Different fixture")
        }
        assertTrue(config.matchesText(expected))
        assertFalse(config.hasExactEditableText(expected))
    }
}
