/* 车端推流示例（C ABI 消费）— mediaservo_field_* 用法。
 *
 * 编译（链接 libmediaservo_field.so）:
 *   gcc vehicle_push.c -I bindings/c/mediaservo-field-c/include -I bindings/c/include \
 *       -L target/debug -lmediaservo_field -o vehicle_push
 * 运行: LD_LIBRARY_PATH=target/debug ./vehicle_push
 */
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include "mediaservo_field.h"

int main(void) {
    /* 1. 配置 */
    mediaservo_push_config_t cfg = MEDIASERVO_PUSH_CONFIG_DEFAULT;
    cfg.url = "ws://127.0.0.1:9800/ws";
    cfg.psk = "mediaservo-dev";
    cfg.room = "vehicle-c";
    cfg.width = 1280;
    cfg.height = 720;
    cfg.framerate = 30;
    cfg.bitrate_kbps = 2000;

    char err[256];

    /* 2. 连接 */
    mediaservo_field_push_t* s = NULL;
    mediaservo_err_t rc = mediaservo_field_push_connect(&cfg, &s);
    if (rc != MEDIASERVO_OK) {
        mediaservo_field_last_error(err, sizeof(err));
        fprintf(stderr, "connect failed (%d): %s\n", rc, err);
        return 1;
    }
    printf("connected\n");

    /* 3. 发布视频轨 */
    char track[64];
    rc = mediaservo_field_push_publish_video(s, track, sizeof(track));
    if (rc != MEDIASERVO_OK) {
        mediaservo_field_last_error(err, sizeof(err));
        fprintf(stderr, "publish failed (%d): %s\n", rc, err);
        mediaservo_field_push_close(s);
        return 1;
    }
    printf("published: track=%s\n", track);

    /* 4. 启动帧生成 */
    rc = mediaservo_field_push_start_video_frames(s);
    if (rc != MEDIASERVO_OK) {
        mediaservo_field_last_error(err, sizeof(err));
        fprintf(stderr, "start frames failed (%d): %s\n", rc, err);
        mediaservo_field_push_close(s);
        return 1;
    }
    printf("frames running\n");

    /* 5. 运行 30s（车端循环） */
    for (int i = 0; i < 30; i++) {
        sleep(1);
        if (i % 10 == 0) printf("t=%ds ...\n", i);
    }

    /* 6. 停止 + 关闭 */
    mediaservo_field_push_stop_video_frames(s);
    rc = mediaservo_field_push_close(s);
    if (rc != MEDIASERVO_OK) {
        mediaservo_field_last_error(err, sizeof(err));
        fprintf(stderr, "close failed (%d): %s\n", rc, err);
        return 1;
    }
    printf("done\n");
    return 0;
}
