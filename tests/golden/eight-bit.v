module Eight_bit_XOR_With_Constant_Mask (
    data_in,
    data_out
);

input wire [7:0] data_in;
output wire [7:0] data_out;

wire [7:0] data_in__2;
wire [7:0] mask_value;
wire [7:0] data_out__2;

assign data_in__2 = data_in;
assign data_out = data_out__2;

assign mask_value = 8'b10100101;
assign data_out__2 = data_in__2 ^ mask_value;

endmodule
