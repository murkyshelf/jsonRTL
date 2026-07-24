module n_123___module (
    data_in,
    data_in__2,
    output_id
);

input wire data_in;
input wire data_in__2;
output wire output_id;

wire data_in__3;
wire data_in__4;
wire output_id__2;

assign data_in__3 = data_in;
assign data_in__4 = data_in__2;
assign output_id = output_id__2;

assign output_id__2 = data_in__3 & data_in__4;

endmodule
