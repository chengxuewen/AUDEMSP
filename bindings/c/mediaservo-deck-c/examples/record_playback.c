/* 采集→录制→回放 闭环示例（C ABI 消费）— ms_deck_* 用法。
 *
 * 编译（链接 libmediaservo_deck.so）:
 *   gcc record_playback.c -I bindings/c/mediaservo-deck-c/include -I bindings/c/include \
 *       -L target/debug -lmediaservo_deck -o record_playback
 * 运行: LD_LIBRARY_PATH=target/debug ./record_playback
 */
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "mediaservo_deck.h"

static long g_camera_frames = 0;
static long g_playback_frames = 0;

static void on_camera_frame(const ms_frame_t* f, void* user) {
    (void)user;
    if (g_camera_frames % 30 == 0)
        printf("[cam] frame %ld: %ux%u pts=%llu stride_y=%u\n", g_camera_frames,
               f->width, f->height, (unsigned long long)f->pts_us, f->stride_y);
    g_camera_frames++;
}

static void on_player_frame(const ms_frame_t* f, void* user) {
    (void)user;
    if (g_playback_frames % 30 == 0)
        printf("[ply] frame %ld: %ux%u pts=%llu\n", g_playback_frames,
               f->width, f->height, (unsigned long long)f->pts_us);
    g_playback_frames++;
}

int main(void) {
    char err[256];
    ms_deck_camera_t* cam = NULL;
    ms_deck_recorder_t* rec = NULL;
    ms_deck_player_t* player = NULL;

    /* 1. 枚举相机（双调用模式: 第一次长度 → 第二次内容） */
    size_t need = 0;
    ms_err_t rc = ms_deck_devices_enumerate(0, NULL, 0, &need);
    if (rc < 0) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "enumerate failed (%d): %s\n", rc, err);
        return 1;
    }
    char dev[64];
    rc = ms_deck_devices_enumerate(0, dev, sizeof(dev), &need);
    if (rc < 0) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "enumerate fill failed (%d): %s\n", rc, err);
        return 1;
    }
    printf("devices (%zu bytes): %s\n", need, dev);

    /* 2. 打开相机 + 开始产帧（默认 1280x720@30）+ 帧回调 */
    ms_deck_capture_options_t copts = MS_DECK_CAPTURE_OPTIONS_DEFAULT;
    rc = ms_deck_camera_open(dev, &copts, &cam);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "camera_open failed (%d): %s\n", rc, err);
        return 1;
    }
    rc = ms_deck_camera_start(cam);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "camera_start failed (%d): %s\n", rc, err);
        return 1;
    }
    rc = ms_deck_camera_frames_cb(cam, on_camera_frame, NULL);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "frames_cb failed (%d): %s\n", rc, err);
        return 1;
    }

    /* 3. 录制 3 秒（camera → recorder 桥接） */
    const char* out_path = "/tmp/opencode/deck_test.mp4";
    rc = ms_deck_recorder_new(out_path, &rec);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "recorder_new failed (%d): %s\n", rc, err);
        return 1;
    }
    rc = ms_deck_recorder_record(rec, cam);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "record failed (%d): %s\n", rc, err);
        return 1;
    }
    printf("recording 3s -> %s ...\n", out_path);
    sleep(3);

    /* 4. 停止录制 + 关闭（顺序: recorder 先于 camera — 生命周期契约） */
    rc = ms_deck_recorder_stop(rec);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "recorder_stop failed (%d): %s\n", rc, err);
        return 1;
    }
    rc = ms_deck_recorder_close(rec);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "recorder_close failed (%d): %s\n", rc, err);
        return 1;
    }
    rc = ms_deck_camera_stop(cam);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "camera_stop failed (%d): %s\n", rc, err);
        return 1;
    }
    rc = ms_deck_camera_close(cam);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "camera_close failed (%d): %s\n", rc, err);
        return 1;
    }
    printf("recorded %ld camera frames -> %s\n", g_camera_frames, out_path);

    /* 5. 回放: 解码逐帧计数 */
    rc = ms_deck_player_open(out_path, &player);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "player_open failed (%d): %s\n", rc, err);
        return 1;
    }
    rc = ms_deck_player_frames_cb(player, on_player_frame, NULL);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "player_frames_cb failed (%d): %s\n", rc, err);
        return 1;
    }
    rc = ms_deck_player_close(player);
    if (rc != MS_OK) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "player_close failed (%d): %s\n", rc, err);
        return 1;
    }
    if (g_playback_frames == 0) {
        ms_deck_last_error(err, sizeof(err));
        fprintf(stderr, "playback produced 0 frames; last_error: %s\n", err);
    }
    printf("playback decoded %ld frames\n", g_playback_frames);

    printf("verify: ffprobe -v error -show_entries format=duration -of csv=p=0 %s\n",
           out_path);
    return 0;
}
