#ifndef BridgingHeader_h
#define BridgingHeader_h

#include <stdbool.h>

void douyin_parser_ios_start(void);
void gpui_ios_request_frame(void *window_ptr);
void *gpui_ios_get_window(void);
void gpui_ios_handle_touch(void *window_ptr, void *touch_ptr, void *event_ptr);
void gpui_ios_will_enter_foreground(void *app_ptr);
void gpui_ios_did_become_active(void *app_ptr);
void gpui_ios_will_resign_active(void *app_ptr);
void gpui_ios_did_enter_background(void *app_ptr);
void gpui_ios_will_terminate(void *app_ptr);

#endif
