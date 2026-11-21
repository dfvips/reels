#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>
#import <Photos/Photos.h>
#import <UIKit/UIKit.h>

/// 重新写出图片并补充 Live Photo 所需的 MakerApple["17"] 标识。
/// ImageIO 无法在不重编码的前提下写入 MakerApple，因此这里以最高质量
/// 重编码图片，保证与原始图视觉一致。
static NSURL *ReelsWriteLivePhotoImage(NSString *imagePath, NSString *identifier, NSString **outUTI) {
    NSData *data = [NSData dataWithContentsOfFile:imagePath];
    if (data.length == 0) {
        NSLog(@"reels: live photo image read failed");
        return nil;
    }
    CGImageSourceRef source = CGImageSourceCreateWithData((__bridge CFDataRef)data, NULL);
    if (source == NULL) {
        NSLog(@"reels: live photo image source failed");
        return nil;
    }
    CFStringRef sourceType = CGImageSourceGetType(source);
    if (sourceType == NULL) {
        NSLog(@"reels: live photo image type unknown");
        CFRelease(source);
        return nil;
    }
    NSString *uti = [NSString stringWithString:(__bridge NSString *)sourceType];

    NSString *extension = imagePath.pathExtension;
    if (extension.length == 0) {
        extension = @"jpg";
    }
    NSString *fileName = [NSString stringWithFormat:@"reels-live-%@.%@", identifier, extension];
    NSURL *outputURL = [NSURL fileURLWithPath:
        [NSTemporaryDirectory() stringByAppendingPathComponent:fileName]];
    [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];

    NSDictionary *sourceProperties = CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, 0, NULL));
    NSMutableDictionary *properties = sourceProperties != nil
        ? [sourceProperties mutableCopy]
        : [NSMutableDictionary dictionary];
    NSMutableDictionary *makerApple = properties[(__bridge NSString *)kCGImagePropertyMakerAppleDictionary]
        != nil
        ? [properties[(__bridge NSString *)kCGImagePropertyMakerAppleDictionary] mutableCopy]
        : [NSMutableDictionary dictionary];
    makerApple[@"17"] = identifier;
    properties[(__bridge NSString *)kCGImagePropertyMakerAppleDictionary] = makerApple;
    properties[(__bridge NSString *)kCGImageDestinationLossyCompressionQuality] = @0.95;

    CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
        (__bridge CFURLRef)outputURL, sourceType, 1, NULL);
    if (destination == NULL) {
        NSLog(@"reels: live photo image destination failed for %@", uti);
        CFRelease(source);
        return nil;
    }
    CGImageDestinationAddImageFromSource(destination, source, 0,
        (__bridge CFDictionaryRef)properties);
    bool written = CGImageDestinationFinalize(destination);
    CFRelease(destination);
    CFRelease(source);
    if (!written) {
        NSLog(@"reels: live photo image write failed for %@", uti);
        [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
        return nil;
    }
    if (outUTI != NULL) {
        *outUTI = uti;
    }
    return outputURL;
}

/// 以 passthrough 方式把动轨封装为 QuickTime .mov，并写入与照片一致的
/// content identifier；音视频采样数据原样复制，不做转码。
static NSURL *ReelsWriteLivePhotoVideo(NSString *videoPath, NSString *identifier) {
    AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:videoPath] options:nil];
    if (asset == nil || [asset tracksWithMediaType:AVMediaTypeVideo].count == 0) {
        return nil;
    }
    NSString *fileName = [NSString stringWithFormat:@"reels-live-%@.mov", identifier];
    NSURL *outputURL = [NSURL fileURLWithPath:
        [NSTemporaryDirectory() stringByAppendingPathComponent:fileName]];
    [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];

    AVAssetExportSession *session = [[AVAssetExportSession alloc]
        initWithAsset:asset
        presetName:AVAssetExportPresetPassthrough];
    if (session == nil) {
        return nil;
    }
    session.outputURL = outputURL;
    session.outputFileType = AVFileTypeQuickTimeMovie;
    session.shouldOptimizeForNetworkUse = NO;
    AVMutableMetadataItem *item = [[AVMutableMetadataItem alloc] init];
    item.keySpace = AVMetadataKeySpaceQuickTimeMetadata;
    item.key = AVMetadataQuickTimeMetadataKeyContentIdentifier;
    item.value = identifier;
    session.metadata = @[ item ];

    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    [session exportAsynchronouslyWithCompletionHandler:^{
        dispatch_semaphore_signal(semaphore);
    }];
    dispatch_semaphore_wait(semaphore, DISPATCH_TIME_FOREVER);

    if (session.status != AVAssetExportSessionStatusCompleted) {
        NSLog(@"reels: live photo video export failed: %@", session.error);
        [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
        return nil;
    }
    return outputURL;
}

static void ReelsCommitLivePhoto(
    NSURL *photoURL,
    NSString *photoUTI,
    NSURL *videoURL,
    NSString *imagePath,
    NSString *videoPath,
    void (^completion)(BOOL success, NSError *error)
) {
    [[PHPhotoLibrary sharedPhotoLibrary] performChanges:^{
        PHAssetCreationRequest *request = [PHAssetCreationRequest creationRequestForAsset];
        PHAssetResourceCreationOptions *photoOptions = [[PHAssetResourceCreationOptions alloc] init];
        if (photoUTI.length > 0) {
            photoOptions.uniformTypeIdentifier = photoUTI;
        }
        [request addResourceWithType:PHAssetResourceTypePhoto
                             fileURL:photoURL
                             options:photoOptions];

        PHAssetResourceCreationOptions *videoOptions = [[PHAssetResourceCreationOptions alloc] init];
        videoOptions.uniformTypeIdentifier = @"com.apple.quicktime-movie";
        [request addResourceWithType:PHAssetResourceTypePairedVideo
                             fileURL:videoURL
                             options:videoOptions];
    } completionHandler:^(BOOL success, NSError *error) {
        if (!success) {
            NSLog(@"reels: live photo save failed: %@", error);
        }
        NSFileManager *fileManager = [NSFileManager defaultManager];
        [fileManager removeItemAtURL:photoURL error:nil];
        [fileManager removeItemAtURL:videoURL error:nil];
        if (success) {
            [fileManager removeItemAtPath:imagePath error:nil];
            [fileManager removeItemAtPath:videoPath error:nil];
        }
        if (completion != nil) {
            completion(success, error);
        }
    }];
}

/// 把系统可解码的图片（HEIC/HEIF/AVIF 等）转成 JPEG；max_dimension <= 0 时保持原尺寸。
bool reels_convert_image_to_jpeg(
    const char *source_path, const char *target_path, int max_dimension, int quality) {
    if (source_path == NULL || target_path == NULL) {
        return false;
    }
    NSString *sourcePath = [NSString stringWithUTF8String:source_path];
    NSString *targetPath = [NSString stringWithUTF8String:target_path];
    NSData *data = [NSData dataWithContentsOfFile:sourcePath];
    if (data.length == 0) {
        return false;
    }
    CGImageSourceRef source = CGImageSourceCreateWithData((__bridge CFDataRef)data, NULL);
    if (source == NULL) {
        return false;
    }
    int limit = max_dimension;
    if (limit <= 0) {
        NSDictionary *properties = CFBridgingRelease(
            CGImageSourceCopyPropertiesAtIndex(source, 0, NULL));
        NSNumber *width = properties[(__bridge NSString *)kCGImagePropertyPixelWidth];
        NSNumber *height = properties[(__bridge NSString *)kCGImagePropertyPixelHeight];
        limit = (int)MAX(width.integerValue, height.integerValue);
    }
    NSDictionary *options = @{
        (__bridge NSString *)kCGImageSourceCreateThumbnailFromImageAlways: @YES,
        (__bridge NSString *)kCGImageSourceCreateThumbnailWithTransform: @YES,
        (__bridge NSString *)kCGImageSourceShouldCacheImmediately: @YES,
        (__bridge NSString *)kCGImageSourceThumbnailMaxPixelSize: @(MAX(limit, 1)),
    };
    CGImageRef image = CGImageSourceCreateThumbnailAtIndex(
        source, 0, (__bridge CFDictionaryRef)options);
    CFRelease(source);
    if (image == NULL) {
        return false;
    }
    NSURL *targetURL = [NSURL fileURLWithPath:targetPath];
    CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
        (__bridge CFURLRef)targetURL, CFSTR("public.jpeg"), 1, NULL);
    if (destination == NULL) {
        CGImageRelease(image);
        return false;
    }
    int clampedQuality = MAX(1, MIN(quality, 100));
    NSDictionary *destinationOptions = @{
        (__bridge NSString *)kCGImageDestinationLossyCompressionQuality: @(clampedQuality / 100.0),
    };
    CGImageDestinationAddImage(
        destination, image, (__bridge CFDictionaryRef)destinationOptions);
    bool written = CGImageDestinationFinalize(destination);
    CFRelease(destination);
    CGImageRelease(image);
    return written;
}

static void ReelsSaveMediaToPhotos(
    NSString *filePath,
    void (^completion)(BOOL success, NSError *error)
) {
    void (^finish)(BOOL, NSError *) = ^(BOOL success, NSError *error) {
        if (success) {
            [[NSFileManager defaultManager] removeItemAtPath:filePath error:nil];
        }
        if (completion != nil) {
            completion(success, error);
        }
    };

    if ([[filePath.pathExtension lowercaseString] isEqualToString:@"heic"] ||
        [[filePath.pathExtension lowercaseString] isEqualToString:@"heif"]) {
        UIImage *image = [UIImage imageWithContentsOfFile:filePath];
        if (image == nil) {
            finish(NO, nil);
            return;
        }
        [[PHPhotoLibrary sharedPhotoLibrary] performChanges:^{
            [PHAssetCreationRequest creationRequestForAssetFromImage:image];
        } completionHandler:^(BOOL success, NSError *error) {
            finish(success, error);
        }];
    } else if ([filePath hasSuffix:@".mp4"] || [filePath hasSuffix:@".m4v"] ||
               [filePath hasSuffix:@".mov"]) {
        [[PHPhotoLibrary sharedPhotoLibrary] performChanges:^{
            [PHAssetCreationRequest creationRequestForAssetFromVideoAtFileURL:
                [NSURL fileURLWithPath:filePath]];
        } completionHandler:^(BOOL success, NSError *error) {
            finish(success, error);
        }];
    } else {
        UIImage *image = [UIImage imageWithContentsOfFile:filePath];
        if (image == nil) {
            finish(NO, nil);
            return;
        }
        [[PHPhotoLibrary sharedPhotoLibrary] performChanges:^{
            [PHAssetCreationRequest creationRequestForAssetFromImage:image];
        } completionHandler:^(BOOL success, NSError *error) {
            finish(success, error);
        }];
    }
}

void reels_save_media_to_photos(const char *path) {
    if (path == NULL) {
        return;
    }
    NSString *filePath = [NSString stringWithUTF8String:path];
    dispatch_async(dispatch_get_main_queue(), ^{
        ReelsSaveMediaToPhotos(filePath, nil);
    });
}

bool reels_save_media_to_photos_wait(const char *path) {
    if (path == NULL) {
        return false;
    }
    NSString *filePath = [NSString stringWithUTF8String:path];
    __block bool saved = false;
    dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
    ReelsSaveMediaToPhotos(filePath, ^(BOOL success, NSError *error) {
        saved = success;
        dispatch_semaphore_signal(semaphore);
    });
    dispatch_semaphore_wait(semaphore, DISPATCH_TIME_FOREVER);
    return saved;
}

static bool ReelsSaveLivePhoto(
    const char *image_path,
    const char *video_path,
    bool wait_for_completion
) {
    if (image_path == NULL || video_path == NULL) {
        return false;
    }
    NSString *imagePath = [NSString stringWithUTF8String:image_path];
    NSString *videoPath = [NSString stringWithUTF8String:video_path];
    if (imagePath.length == 0 || videoPath.length == 0) {
        return false;
    }
    @autoreleasepool {
        NSString *identifier = [[NSUUID UUID] UUIDString];
        NSString *photoUTI = nil;
        NSURL *photoURL = ReelsWriteLivePhotoImage(imagePath, identifier, &photoUTI);
        if (photoURL == nil) {
            NSLog(@"reels: live photo image packaging failed");
            return false;
        }
        NSURL *videoURL = ReelsWriteLivePhotoVideo(videoPath, identifier);
        if (videoURL == nil) {
            [[NSFileManager defaultManager] removeItemAtURL:photoURL error:nil];
            return false;
        }
        if (!wait_for_completion) {
            ReelsCommitLivePhoto(photoURL, photoUTI, videoURL, imagePath, videoPath, nil);
            return true;
        }
        __block bool saved = false;
        dispatch_semaphore_t semaphore = dispatch_semaphore_create(0);
        ReelsCommitLivePhoto(
            photoURL,
            photoUTI,
            videoURL,
            imagePath,
            videoPath,
            ^(BOOL success, NSError *error) {
                saved = success;
                dispatch_semaphore_signal(semaphore);
            }
        );
        dispatch_semaphore_wait(semaphore, DISPATCH_TIME_FOREVER);
        return saved;
    }
    return false;
}

bool reels_save_live_photo_to_photos(const char *image_path, const char *video_path) {
    return ReelsSaveLivePhoto(image_path, video_path, false);
}

bool reels_save_live_photo_to_photos_wait(const char *image_path, const char *video_path) {
    return ReelsSaveLivePhoto(image_path, video_path, true);
}
