package me.go_gradually.omypic.infrastructure.stt.job;

import me.go_gradually.omypic.application.stt.model.SttJob;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

class InMemorySttJobStoreTest {

    @Test
    void createAndGet_returnsSameJob() {
        InMemorySttJobStore store = new InMemorySttJobStore();

        SttJob created = store.create("j1", "s1");
        SttJob loaded = store.get("j1");

        assertSame(created, loaded);
        assertEquals("s1", loaded.getSessionId());
    }

    @Test
    void get_returnsNull_whenUnknownJobId() {
        InMemorySttJobStore store = new InMemorySttJobStore();

        assertNull(store.get("missing"));
    }
}
