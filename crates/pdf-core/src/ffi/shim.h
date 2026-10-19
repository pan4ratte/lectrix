/*
 * Shared by the Lectrix FFI shim's C files (shim.c, compress.c).
 */
#ifndef LECTRIX_SHIM_H
#define LECTRIX_SHIM_H

#include "mupdf/fitz.h"

typedef struct lectrix_error {
	int code;
	char message[256];
} lectrix_error;

/* Copies the error MuPDF caught into `err`; returns a nonzero code for the caller. */
int lectrix_caught(fz_context *ctx, lectrix_error *err);

#endif
