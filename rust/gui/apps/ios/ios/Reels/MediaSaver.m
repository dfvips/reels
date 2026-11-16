#import <Foundation/Foundation.h>
#import <Photos/Photos.h>
#import <UIKit/UIKit.h>

void reels_save_media_to_photos(const char *path) {
    if (path == NULL) {
        return;
    }
    NSString *filePath = [NSString stringWithUTF8String:path];
    dispatch_async(dispatch_get_main_queue(), ^{
        if ([[filePath.pathExtension lowercaseString] isEqualToString:@"heic"] ||
            [[filePath.pathExtension lowercaseString] isEqualToString:@"heif"]) {
            UIImage *image = [UIImage imageWithContentsOfFile:filePath];
            if (image == nil) {
                return;
            }
            [[PHPhotoLibrary sharedPhotoLibrary] performChanges:^{
                [PHAssetCreationRequest creationRequestForAssetFromImage:image];
            } completionHandler:^(BOOL success, NSError *error) {
                if (success) {
                    dispatch_async(dispatch_get_global_queue(QOS_CLASS_BACKGROUND, 0), ^{
                        [[NSFileManager defaultManager] removeItemAtPath:filePath error:nil];
                    });
                }
            }];
        } else if ([filePath hasSuffix:@".mp4"] || [filePath hasSuffix:@".m4v"] ||
                   [filePath hasSuffix:@".mov"]) {
            [[PHPhotoLibrary sharedPhotoLibrary] performChanges:^{
                [PHAssetCreationRequest creationRequestForAssetFromVideoAtFileURL:
                    [NSURL fileURLWithPath:filePath]];
            } completionHandler:^(BOOL success, NSError *error) {
                if (success) {
                    dispatch_async(dispatch_get_global_queue(QOS_CLASS_BACKGROUND, 0), ^{
                        [[NSFileManager defaultManager] removeItemAtPath:filePath error:nil];
                    });
                }
            }];
        } else {
            UIImage *image = [UIImage imageWithContentsOfFile:filePath];
            if (image != nil) {
                [[PHPhotoLibrary sharedPhotoLibrary] performChanges:^{
                    [PHAssetCreationRequest creationRequestForAssetFromImage:image];
                } completionHandler:^(BOOL success, NSError *error) {
                    if (success) {
                        dispatch_async(dispatch_get_global_queue(QOS_CLASS_BACKGROUND, 0), ^{
                            [[NSFileManager defaultManager] removeItemAtPath:filePath error:nil];
                        });
                    }
                }];
            }
        }
    });
}
