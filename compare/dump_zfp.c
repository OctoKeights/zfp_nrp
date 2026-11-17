/* The C side of the comparison with the zfp library (https://github.com/LLNL/zfp) v1.0.1:
   sets up streams in every mode with synthetic settings, codes synthetic 4 x 4 blocks of
   floats into them, decodes them intact, damaged and random, and prints a running hash of
   the output. examples/dump.rs prints the same from zfp_nrp, and compare/compare.sh compares
   the two.

     dump zfp <seed> <count>   synthetic settings in every mode, with the maximum stream size
                               of a random field; blocks of eight kinds coded with them; each
                               stream decoded intact, with bits flipped, and random words
                               decoded */
#include "zfp.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The two programs make the same data from the same seed: a 64-bit linear congruential
   generator, the high half of its state. */
static uint64_t g_state;
static uint32_t Next(void)
{
    g_state = g_state * 6364136223846793005ull + 1442695040888963407ull;
    return (uint32_t)(g_state >> 32);
}

/* FNV-1a, 64 bits. */
static uint64_t g_hash = 14695981039346656037ull;
static void Add(const void *data, size_t size)
{
    const unsigned char *bytes = (const unsigned char *)data;
    size_t i;
    for (i = 0; i < size; ++i)
    {
        g_hash ^= bytes[i];
        g_hash *= 1099511628211ull;
    }
}
static void AddNumber(uint32_t value)
{
    unsigned char bytes[4];
    bytes[0] = (unsigned char)value;
    bytes[1] = (unsigned char)(value >> 8);
    bytes[2] = (unsigned char)(value >> 16);
    bytes[3] = (unsigned char)(value >> 24);
    Add(bytes, 4);
}
static void AddDouble(double value)
{
    uint64_t bits;
    unsigned char bytes[8];
    int i;
    memcpy(&bits, &value, 8);
    for (i = 0; i < 8; ++i)
        bytes[i] = (unsigned char)(bits >> (8 * i));
    Add(bytes, 8);
}

static double DoubleOf(uint64_t bits)
{
    double value;
    memcpy(&value, &bits, 8);
    return value;
}
static float FloatOf(uint32_t bits)
{
    float value;
    memcpy(&value, &bits, 4);
    return value;
}
static uint32_t BitsOf(float value)
{
    uint32_t bits;
    memcpy(&bits, &value, 4);
    return bits;
}

/* Two to e, for e from -1074 to 1023. */
static double PowerOfTwo(int e)
{
    return e >= -1022 ? DoubleOf((uint64_t)(e + 1023) << 52) : DoubleOf(1ull << (e + 1074));
}

static double MakeRate(void)
{
    uint32_t lo, hi;
    switch (Next() % 8)
    {
        case 0: return (double)(Next() % 64) / 4.0;
        case 1: return (double)(Next() % 2000) / 16.0 - 10.0;
        case 2: lo = Next(); hi = Next(); return DoubleOf((uint64_t)lo | ((uint64_t)hi << 32));
        case 3: return 1e10 * (double)(Next() % 10);
        case 4: return 0.5 + (double)(Next() % 100) * 0.01;
        case 5: return (double)(Next() % 33);
        case 6: return -1.5;
        default: return (double)(Next() % 1100);
    }
}

static double MakeTolerance(void)
{
    uint32_t lo, hi;
    int e;
    switch (Next() % 8)
    {
        case 0: return 0.0;
        case 1: e = (int)(Next() % 60) - 30; return PowerOfTwo(e) * (1.0 + (double)(Next() % 100) / 100.0);
        case 2: lo = Next(); hi = Next(); return DoubleOf((uint64_t)lo | ((uint64_t)hi << 32));
        case 3: lo = Next(); hi = Next(); return DoubleOf((uint64_t)lo | ((uint64_t)(hi & 0xfffff) << 32));
        case 4: return -1.0;
        case 5: return DoubleOf(0x7ff0000000000000ull);
        case 6: e = -(int)(Next() % 1074); return PowerOfTwo(e);
        default: return (double)(Next() % 1000) * 1e-6;
    }
}

/* A block of one of eight kinds. */
static void MakeBlock(float *block)
{
    uint32_t a, b;
    int i, j, k;
    switch (Next() % 8)
    {
        case 0:
            a = Next() % 2 ? 0x80000000u : 0;
            for (k = 0; k < 16; ++k)
                block[k] = FloatOf(a);
            break;
        case 1:
        {
            const int ia = (int)(Next() % 2001) - 1000;
            const int ib = (int)(Next() % 201) - 100;
            const int ic = (int)(Next() % 201) - 100;
            const int e = (int)(Next() % 60) - 30;
            const float s = FloatOf((uint32_t)(e + 127) << 23);
            for (j = 0; j < 4; ++j)
                for (i = 0; i < 4; ++i)
                    block[j * 4 + i] = (float)(ia + ib * i + ic * j) * s;
            break;
        }
        case 2:
            for (k = 0; k < 16; ++k)
                block[k] = FloatOf(Next());
            break;
        case 3:
            for (k = 0; k < 16; ++k)
            {
                a = Next();
                b = Next();
                block[k] = FloatOf((a & 0x807fffffu) | ((100 + b % 55) << 23));
            }
            break;
        case 4:
            for (k = 0; k < 16; ++k)
                block[k] = FloatOf(Next() & 0x807fffffu);
            break;
        case 5:
            for (k = 0; k < 16; ++k)
                block[k] = 0.0f;
            k = (int)(Next() % 16);
            a = Next();
            b = Next();
            block[k] = FloatOf((a & 0x807fffffu) | ((1 + b % 254) << 23));
            break;
        case 6:
            for (k = 0; k < 16; ++k)
            {
                a = Next();
                b = Next();
                block[k] = FloatOf((a & 0x807fffffu) | ((250 + b % 5) << 23));
            }
            break;
        default:
            for (k = 0; k < 16; ++k)
                block[k] = (float)((int)(Next() % 9) - 4);
            break;
    }
}

/* The words of a stream, with room past them that is all zeros (the port reads zeros there). */
#define SLACK 4096

static void DecodeAll(zfp_stream *zfp, const uint8_t *bytes, size_t size, size_t words, size_t nblocks)
{
    uint8_t *buffer = (uint8_t *)calloc(words * 8 + SLACK, 1);
    bitstream *s;
    float block[16];
    size_t b;
    int k;
    memcpy(buffer, bytes, size);
    s = stream_open(buffer, words * 8);
    zfp_stream_set_bit_stream(zfp, s);
    zfp_stream_rewind(zfp);
    for (b = 0; b < nblocks; ++b)
    {
        AddNumber((uint32_t)zfp_decode_block_float_2(zfp, block));
        for (k = 0; k < 16; ++k)
            AddNumber(BitsOf(block[k]));
    }
    AddNumber((uint32_t)stream_rtell(s));
    stream_close(s);
    free(buffer);
}

int main(int argc, char **argv)
{
    uint32_t count, round;
    uint64_t total = 0;
    if (argc < 4 || strcmp(argv[1], "zfp") != 0)
    {
        fprintf(stderr, "usage: dump zfp <seed> <count>\n");
        return 2;
    }
    g_state = strtoull(argv[2], NULL, 10);
    count = (uint32_t)strtoul(argv[3], NULL, 10);
    for (round = 0; round < count; ++round)
    {
        zfp_stream *zfp = zfp_stream_open(NULL);
        const uint32_t mode = Next() % 4;
        size_t nx, ny, nblocks, words, b;
        zfp_field *field;
        AddNumber(mode);
        if (mode == 0)
        {
            const double rate = MakeRate();
            AddDouble(zfp_stream_set_rate(zfp, rate, zfp_type_float, 2, zfp_false));
        }
        else if (mode == 1)
        {
            const uint32_t precision = Next() % 70;
            AddNumber(zfp_stream_set_precision(zfp, precision));
        }
        else if (mode == 2)
        {
            const double tolerance = MakeTolerance();
            AddDouble(zfp_stream_set_accuracy(zfp, tolerance));
        }
        AddNumber(zfp->minbits);
        AddNumber(zfp->maxbits);
        AddNumber(zfp->maxprec);
        AddNumber((uint32_t)zfp->minexp);
        nx = Next() % 300;
        ny = Next() % 300;
        field = zfp_field_2d(NULL, zfp_type_float, nx, ny);
        AddNumber((uint32_t)zfp_stream_maximum_size(zfp, field));
        zfp_field_free(field);

        nblocks = 1 + Next() % 40;
        field = zfp_field_2d(NULL, zfp_type_float, 4 * nblocks, 4);
        words = zfp_stream_maximum_size(zfp, field) / 8;
        zfp_field_free(field);
        /* (Settings of more than eight megabytes a stream are left at that.) */
        if (words <= (1u << 20))
        {
            uint8_t *buffer = (uint8_t *)calloc(words * 8 + SLACK, 1);
            bitstream *s = stream_open(buffer, words * 8);
            float block[16];
            size_t size;
            zfp_stream_set_bit_stream(zfp, s);
            zfp_stream_rewind(zfp);
            for (b = 0; b < nblocks; ++b)
            {
                MakeBlock(block);
                AddNumber((uint32_t)zfp_encode_block_float_2(zfp, block));
            }
            AddNumber((uint32_t)zfp_stream_flush(zfp));
            size = stream_size(s);
            AddNumber((uint32_t)size);
            Add(buffer, size);
            stream_close(s);
            DecodeAll(zfp, buffer, size, words, nblocks);

            if (Next() % 2 == 0 && size > 0)
            {
                const uint32_t flips = 1 + Next() % 4;
                uint32_t f;
                for (f = 0; f < flips; ++f)
                {
                    const uint32_t bit = Next() % (uint32_t)(size * 8);
                    buffer[bit >> 3] ^= (uint8_t)(1u << (bit & 7));
                }
                DecodeAll(zfp, buffer, size, words, nblocks);
            }
            if (round % 4 == 0)
            {
                size_t i;
                for (i = 0; i < words * 8; ++i)
                    buffer[i] = (uint8_t)Next();
                DecodeAll(zfp, buffer, words * 8, words, nblocks);
            }
            free(buffer);
            total += nblocks;
        }
        zfp_stream_close(zfp);
        if ((round + 1) % 200 == 0 || round + 1 == count)
            printf("zfp %u %016llx\n", round + 1, (unsigned long long)g_hash);
    }
    printf("zfp blocks: %llu\n", (unsigned long long)total);
    return 0;
}
