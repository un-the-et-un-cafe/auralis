package chat.omcp.app

import androidx.compose.ui.semantics.SemanticsConfiguration
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.style.TextDecoration
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Pure assertion regressions: no Activity rule and no changes to the app's smoke-test state. */
class EditableTextAssertionTest {
    private val expected = "Draft survives rotation"

    private fun editor(value: AnnotatedString) = SemanticsConfiguration().apply {
        this[SemanticsProperties.EditableText] = value
    }

    @Test fun exactTextAcceptsPlainAndImeUnderlinedText() {
        val plain = AnnotatedString(expected)
        val underlined = AnnotatedString(
            expected,
            spanStyles = listOf(AnnotatedString.Range(
                SpanStyle(textDecoration = TextDecoration.Underline), 15, expected.length,
            )),
        )
        // Reproduce the old matcher failure despite identical text.
        assertNotEquals(plain, underlined)
        assertTrue(editor(plain).hasExactEditableText(expected))
        assertTrue(editor(underlined).hasExactEditableText(expected))
    }

    @Test fun exactTextRejectsMissingEmptyAndChangedDrafts() {
        for (changed in listOf("", "Draft survives rotatio", "$expected!", " $expected", "$expected ", "draft survives rotation")) {
            assertFalse("Must reject '$changed'", editor(AnnotatedString(changed)).hasExactEditableText(expected))
        }
        val labelOnly = SemanticsConfiguration().apply {
            this[SemanticsProperties.Text] = listOf(AnnotatedString(expected))
        }
        assertFalse(SemanticsConfiguration().hasExactEditableText(expected))
        assertFalse(labelOnly.hasExactEditableText(expected))
    }
}
