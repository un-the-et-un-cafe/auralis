package chat.omcp.app

import chat.omcp.app.core.Contact
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.ui.conversationContacts
import org.junit.Assert.assertEquals
import org.junit.Test

class ConversationPresentationTest {
    private fun contact(id: String, name: String) = Contact(id, name, emptyList(), false)
    private fun message(contactId: String, time: Long, text: String = "private text") =
        ChatMessage("$contactId-$time", contactId, text, true, time, ChatMessage.Status.Queued)

    @Test fun `latest activity sorts first even when messages arrive out of order`() {
        val contacts = listOf(contact("a", "Alice"), contact("b", "Bob"), contact("c", "Camille"))
        val messages = mapOf("a" to listOf(message("a", 100), message("a", 5)), "b" to listOf(message("b", 90)))
        assertEquals(listOf("a", "b", "c"), conversationContacts(contacts, messages, "").map { it.id })
    }

    @Test fun `new contacts have deterministic case insensitive name ordering`() {
        val contacts = listOf(contact("c", "Zoé"), contact("b", "alice"), contact("a", "Alice"))
        assertEquals(listOf("a", "b", "c"), conversationContacts(contacts, emptyMap(), " ").map { it.id })
    }

    @Test fun `search matches contact names without searching private or burn contents`() {
        val contacts = listOf(contact("a", "Alice"), contact("b", "Bob"))
        val messages = mapOf("b" to listOf(message("b", 100, "Alice is here")))
        assertEquals(listOf("a"), conversationContacts(contacts, messages, " ALI ").map { it.id })
        assertEquals(emptyList<Contact>(), conversationContacts(contacts, messages, "here"))
    }

    @Test fun `orphan history never creates a phantom contact`() {
        assertEquals(emptyList<Contact>(), conversationContacts(emptyList(), mapOf("unknown" to listOf(message("unknown", 100))), ""))
    }
}
