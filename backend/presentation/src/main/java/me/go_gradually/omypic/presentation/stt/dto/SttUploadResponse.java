package me.go_gradually.omypic.presentation.stt.dto;

public class SttUploadResponse {
    private String text;
    private String jobId;

    public static SttUploadResponse forText(String text) {
        SttUploadResponse response = new SttUploadResponse();
        response.setText(text);
        return response;
    }

    public static SttUploadResponse forJob(String jobId) {
        SttUploadResponse response = new SttUploadResponse();
        response.setJobId(jobId);
        return response;
    }

    public String getText() {
        return text;
    }

    public void setText(String text) {
        this.text = text;
    }

    public String getJobId() {
        return jobId;
    }

    public void setJobId(String jobId) {
        this.jobId = jobId;
    }
}
