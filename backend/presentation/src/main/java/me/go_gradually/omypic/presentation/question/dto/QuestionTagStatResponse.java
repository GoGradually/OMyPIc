package me.go_gradually.omypic.presentation.question.dto;

public class QuestionTagStatResponse {
    private String tag;
    private long groupCount;
    private boolean selectable;

    public String getTag() {
        return tag;
    }

    public void setTag(String tag) {
        this.tag = tag;
    }

    public long getGroupCount() {
        return groupCount;
    }

    public void setGroupCount(long groupCount) {
        this.groupCount = groupCount;
    }

    public boolean isSelectable() {
        return selectable;
    }

    public void setSelectable(boolean selectable) {
        this.selectable = selectable;
    }
}
