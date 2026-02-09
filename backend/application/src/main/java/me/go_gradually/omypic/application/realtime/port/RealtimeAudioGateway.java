package me.go_gradually.omypic.application.realtime.port;

import me.go_gradually.omypic.application.realtime.model.RealtimeAudioEventListener;
import me.go_gradually.omypic.application.realtime.model.RealtimeAudioOpenCommand;
import me.go_gradually.omypic.application.realtime.model.RealtimeAudioSession;

public interface RealtimeAudioGateway {
    RealtimeAudioSession open(RealtimeAudioOpenCommand command, RealtimeAudioEventListener listener);
}
