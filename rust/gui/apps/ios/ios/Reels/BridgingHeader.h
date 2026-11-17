#ifndef BridgingHeader_h
#define BridgingHeader_h

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

void reels_ios_start(void);
void reels_ios_refresh_system_appearance(bool system_dark);
bool reels_ios_app_theme_is_dark(bool system_dark);
void reels_save_media_to_photos(const char *path);
bool reels_save_live_photo_to_photos(const char *image_path, const char *video_path);
void gpui_ios_request_frame(void *window_ptr);
void *gpui_ios_get_window(void);
void gpui_ios_handle_touch(void *window_ptr, void *touch_ptr, void *event_ptr);
void gpui_ios_will_enter_foreground(void *app_ptr);
void gpui_ios_did_become_active(void *app_ptr);
void gpui_ios_will_resign_active(void *app_ptr);
void gpui_ios_did_enter_background(void *app_ptr);
void gpui_ios_will_terminate(void *app_ptr);

void reels_native_text_input_changed(
    const char *id,
    const char *text,
    size_t selection_start,
    size_t selection_length
);
void reels_native_text_input_selection_changed(
    const char *id,
    size_t selection_start,
    size_t selection_length
);
void reels_native_text_input_height_changed(
    const char *id,
    const char *text,
    float height
);
void reels_native_text_input_focus_changed(const char *id, int32_t focused);
void reels_ios_native_input_paste(const char *id);

#endif
