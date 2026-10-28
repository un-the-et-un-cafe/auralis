package chat.omcp.app.ui

import chat.omcp.app.core.Contact
import chat.omcp.app.data.ChatMessage

/** Only contact names are searched; burn content and message bodies never enter an index. */
fun conversationContacts(
    contacts: List<Contact>,
    messages: Map<String, List<ChatMessage>>,
    query: String,
): List<Contact> {
    val term = query.trim()
    return contacts
        .filter { term.isEmpty() || it.displayName.contains(term, ignoreCase = true) }
        .sortedWith(
            compareByDescending<Contact> { contact -> messages[contact.id]?.maxOfOrNull { it.timeMs } ?: Long.MIN_VALUE }
                .thenBy(String.CASE_INSENSITIVE_ORDER) { it.displayName }
                .thenBy { it.id },
        )
}
