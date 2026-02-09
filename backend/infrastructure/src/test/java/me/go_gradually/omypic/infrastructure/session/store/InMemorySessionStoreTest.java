package me.go_gradually.omypic.infrastructure.session.store;

import me.go_gradually.omypic.domain.session.SessionId;
import me.go_gradually.omypic.domain.session.SessionState;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;

class InMemorySessionStoreTest {

    @Test
    void getOrCreate_returnsSameStateForSameSessionId() {
        InMemorySessionStore store = new InMemorySessionStore();

        SessionState first = store.getOrCreate(SessionId.of("s1"));
        first.appendSegment("hello");
        SessionState second = store.getOrCreate(SessionId.of("s1"));

        assertSame(first, second);
        assertEquals(1, second.getSttSegments().size());
        assertEquals("hello", second.getSttSegments().get(0));
    }
}
